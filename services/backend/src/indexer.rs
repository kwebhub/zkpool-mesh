//! Background indexer.
//!
//! Polls Solana RPC for new transactions that mention our program, parses
//! `DepositEvent` and `WithdrawEvent` from transaction logs, stores them in
//! Postgres, and updates the incremental Merkle tree.
//!
//! ## Polling strategy
//!
//! Every `INDEXER_POLL_INTERVAL_SECS` seconds:
//!   1. Call `getSignaturesForAddress(program_id, { before?, limit })`.
//!   2. Sort ascending (oldest first).
//!   3. For each signature, call `getTransaction(signature)`.
//!   4. Parse `Program data:` log entries, match against known event
//!      discriminators.
//!   5. On match: save to DB, update the tree, emit a metric.
//!   6. Advance `last_processed` **only** on success. On failure, retry
//!      the same signature on the next tick (bounded by `MAX_ATTEMPTS`).
//!
//! ## Deduplication
//!
//! The indexer can re-process the same transaction after an RPC rewind
//! or a restart. All DB writes are idempotent (`ON CONFLICT DO NOTHING`),
//! and the tree is only updated when a commitment is genuinely new.
//!
//! ## Event payload layout (after the 8-byte discriminator)
//!
//! DepositEvent (80 bytes):
//!   commitment      [u8; 32]    (offset 0)
//!   leaf_index      u64 LE      (offset 32)
//!   new_root        [u8; 32]    (offset 40)
//!   timestamp       i64 LE      (offset 72)
//!
//! WithdrawEvent (80 bytes):
//!   nullifier_hash  [u8; 32]    (offset 0)
//!   recipient       Pubkey (32) (offset 32)
//!   amount          u64 LE      (offset 64)
//!   timestamp       i64 LE      (offset 72)

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tokio::sync::Mutex;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::cache::Cache;
use crate::config::Config;
use crate::db::Db;
use crate::metrics;
use crate::tree::MerkleTree;

/// DepositEvent discriminator (from the IDL).
pub const DEPOSIT_EVENT_DISCRIMINATOR: [u8; 8] = [120, 248, 61, 83, 31, 142, 107, 144];

/// WithdrawEvent discriminator (from the IDL).
pub const WITHDRAW_EVENT_DISCRIMINATOR: [u8; 8] = [22, 9, 133, 26, 160, 44, 71, 192];

/// Redis key for the last processed signature.
const LAST_SIGNATURE_KEY: &str = "indexer:last_signature";

/// Redis key for the per-signature attempt counter (failure tracking).
const ATTEMPTS_KEY_PREFIX: &str = "indexer:attempts:";

/// Maximum attempts to process the same signature before skipping it.
const MAX_ATTEMPTS: u32 = 5;

/// RPC retry policy.
const RPC_RETRIES: u32 = 3;
const RPC_RETRY_DELAY_MS: u64 = 500;

/// Parsed DepositEvent.
#[derive(Debug, Clone)]
pub struct DepositEvent {
    pub commitment: [u8; 32],
    pub leaf_index: u64,
    pub new_root: [u8; 32],
    pub timestamp: i64,
}

/// Parsed WithdrawEvent.
#[derive(Debug, Clone)]
pub struct WithdrawEvent {
    pub nullifier_hash: [u8; 32],
    pub recipient: [u8; 32],
    pub amount: u64,
    pub timestamp: i64,
}

/// Background indexer handle.
pub struct Indexer {
    config: Arc<Config>,
    db: Db,
    cache: Arc<Mutex<Cache>>,
    tree: Arc<Mutex<MerkleTree>>,
    http: reqwest::Client,
}

impl Indexer {
    /// Create a new indexer.
    pub fn new(
        config: Arc<Config>,
        db: Db,
        cache: Arc<Mutex<Cache>>,
        tree: Arc<Mutex<MerkleTree>>,
    ) -> Self {
        Self {
            config,
            db,
            cache,
            tree,
            http: reqwest::Client::new(),
        }
    }

    /// Run the indexer loop. Never returns (unless a fatal error).
    pub async fn run(self) -> Result<()> {
        info!(
            "indexer started: program={}, pool={}, interval={}s",
            self.config.solana_rpc_url,
            self.config.pool_address,
            self.config.indexer_poll_interval_secs,
        );

        let mut ticker = interval(Duration::from_secs(self.config.indexer_poll_interval_secs));

        loop {
            ticker.tick().await;
            if let Err(e) = self.poll_once().await {
                error!("indexer poll error: {:#}", e);
                metrics::record_indexer_error();
            }
        }
    }

    /// Perform one polling cycle.
    async fn poll_once(&self) -> Result<()> {
        let program_id = &self.config.program_id;

        let last_processed = {
            let mut cache = self.cache.lock().await;
            cache.get(LAST_SIGNATURE_KEY).await?
        };

        let mut before: Option<String> = None;
        let mut all_signatures: Vec<String> = Vec::new();

        for _page in 0..self.config.indexer_max_pages {
            let sigs = self.fetch_signatures(program_id, before.as_deref()).await?;
            if sigs.is_empty() {
                break;
            }
            before = Some(sigs.last().unwrap().clone());

            let mut stop = false;
            for sig in sigs {
                if Some(&sig) == last_processed.as_ref() {
                    stop = true;
                    break;
                }
                all_signatures.push(sig);
            }
            if stop {
                break;
            }
        }

        if all_signatures.is_empty() {
            debug!("indexer: no new transactions");
            return Ok(());
        }

        // Process oldest first.
        all_signatures.reverse();

        for sig in &all_signatures {
            match self.process_transaction(sig).await {
                Ok(()) => {
                    // Success: advance last_processed and clear attempts.
                    {
                        let mut cache = self.cache.lock().await;
                        cache.set(LAST_SIGNATURE_KEY, sig).await?;
                        cache.del(&format!("{}{}", ATTEMPTS_KEY_PREFIX, sig)).await?;
                    }
                }
                Err(e) => {
                    warn!("failed to process {}: {:#}", sig, e);
                    metrics::record_indexer_error();

                    // Bounded retry: after MAX_ATTEMPTS, skip the signature
                    // so the indexer can make progress on later txs.
                    let attempts_key = format!("{}{}", ATTEMPTS_KEY_PREFIX, sig);
                    let attempts = {
                        let mut cache = self.cache.lock().await;
                        let n: u32 = cache
                            .get(&attempts_key)
                            .await?
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        let n = n + 1;
                        cache.set(&attempts_key, &n.to_string()).await?;
                        n
                    };
                    if attempts >= MAX_ATTEMPTS {
                        warn!(
                            "skipping {} after {} attempts (last error: {:#})",
                            sig, attempts, e
                        );
                        let mut cache = self.cache.lock().await;
                        cache.set(LAST_SIGNATURE_KEY, sig).await?;
                        cache.del(&attempts_key).await?;
                    }
                    // Stop this cycle: keep last_processed where it was so
                    // the same batch is retried next tick.
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    /// Fetch a page of signatures via `getSignaturesForAddress`.
    async fn fetch_signatures(
        &self,
        program_id: &str,
        before: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut params = serde_json::json!([
            program_id,
            {
                "limit": self.config.indexer_page_size,
            }
        ]);
        if let Some(b) = before {
            params[1]["before"] = serde_json::json!(b);
        }

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignaturesForAddress",
            "params": params,
        });

        let resp = self.rpc_call(&body).await?;
        let arr = resp
            .get("result")
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow!("RPC response missing `result` array: {:?}", resp))?;

        let mut out = Vec::with_capacity(arr.len());
        for item in arr {
            if let Some(sig) = item.get("signature").and_then(|s| s.as_str()) {
                out.push(sig.to_string());
            }
        }
        Ok(out)
    }

    /// Fetch a transaction and parse events from its logs.
    ///
    /// Returns `Ok(())` in three cases:
    ///   - transaction found and processed (or had no matching events);
    ///   - transaction explicitly reported as `null` by RPC (skipped);
    ///   - transaction found but logs had no `Program data:` entries.
    async fn process_transaction(&self, signature: &str) -> Result<()> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": [
                signature,
                { "encoding": "json", "maxSupportedTransactionVersion": 0 }
            ],
        });

        let resp = self.rpc_call(&body).await?;

        let result = match resp.get("result") {
            Some(r) if !r.is_null() => r,
            _ => {
                // RPC does not have this transaction (yet). Treat as
                // non-fatal: the signature has been seen in
                // `getSignaturesForAddress` but not yet indexed. Retry on
                // the next tick; the bounded-attempt counter prevents an
                // infinite loop.
                warn!("transaction {} not found on RPC (will retry)", signature);
                return Err(anyhow!("transaction not found: {}", signature));
            }
        };

        let logs = result
            .get("meta")
            .and_then(|m| m.get("logMessages"))
            .and_then(|l| l.as_array());

        let logs = match logs {
            Some(l) => l,
            None => return Ok(()),
        };

        for log in logs {
            let s = match log.as_str() {
                Some(s) => s,
                None => continue,
            };
            if let Some(b64) = s.strip_prefix("Program data: ") {
                if let Ok(bytes) =
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64.trim())
                {
                    self.try_parse_event(signature, &bytes).await?;
                }
            }
        }

        Ok(())
    }

    /// POST a JSON-RPC request with retries.
    async fn rpc_call(&self, body: &serde_json::Value) -> Result<serde_json::Value> {
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..RPC_RETRIES {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(RPC_RETRY_DELAY_MS * attempt as u64))
                    .await;
            }
            match self
                .http
                .post(&self.config.solana_rpc_url)
                .json(body)
                .send()
                .await
            {
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(v) => return Ok(v),
                    Err(e) => last_err = Some(anyhow!("RPC response not JSON: {:#}", e)),
                },
                Err(e) => last_err = Some(anyhow!("RPC request failed: {:#}", e)),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("RPC failed")))
    }

    /// Try to parse a single `Program data:` payload as a DepositEvent or
    /// WithdrawEvent. Silently ignores anything else.
    async fn try_parse_event(&self, signature: &str, bytes: &[u8]) -> Result<()> {
        if bytes.len() < 8 {
            return Ok(());
        }
        let disc = &bytes[..8];
        let payload = &bytes[8..];

        if disc == DEPOSIT_EVENT_DISCRIMINATOR {
            if payload.len() < 80 {
                warn!("DepositEvent payload too short: {} bytes", payload.len());
                return Ok(());
            }
            let evt = parse_deposit_event(payload)?;
            self.handle_deposit(signature, evt).await?;
            return Ok(());
        }

        if disc == WITHDRAW_EVENT_DISCRIMINATOR {
            if payload.len() < 80 {
                warn!("WithdrawEvent payload too short: {} bytes", payload.len());
                return Ok(());
            }
            let evt = parse_withdraw_event(payload)?;
            self.handle_withdraw(signature, evt).await?;
            return Ok(());
        }

        Ok(())
    }

    /// Handle a parsed DepositEvent: save to DB, update tree.
    async fn handle_deposit(&self, signature: &str, evt: DepositEvent) -> Result<()> {
        let already_in_db = self.db.commitment_exists(&evt.commitment).await?;

        self.db
            .save_commitment(
                evt.leaf_index as i64,
                &evt.commitment,
                &self.config.pool_address,
                Some(signature),
            )
            .await?;

        self.db
            .save_root(
                &evt.new_root,
                evt.leaf_index as i64,
                &self.config.pool_address,
                Some(signature),
            )
            .await?;

        // Only update the incremental tree on first sight. Re-processing
        // the same commitment would corrupt the tree (it would be added
        // again at the next leaf position).
        if !already_in_db {
            let leaf_hex = hex::encode(evt.commitment);
            let mut tree = self.tree.lock().await;
            match tree.add_leaf(&leaf_hex).await {
                Ok(root) => {
                    debug!(
                        "tree updated: leaf_index={}, new_root={}",
                        evt.leaf_index, root
                    );
                    let size = tree.size().await.unwrap_or(0);
                    metrics::set_tree_size(size as f64);
                }
                Err(e) => {
                    warn!("tree update failed: {:#}", e);
                    metrics::record_tree_error();
                }
            }
        } else {
            debug!(
                "commitment already in DB, tree untouched: leaf_index={}",
                evt.leaf_index
            );
        }

        metrics::record_deposit();
        let leaf_hex = hex::encode(evt.commitment);
        info!(
            "deposit indexed: leaf_index={}, commitment={}",
            evt.leaf_index,
            &leaf_hex[..16]
        );

        Ok(())
    }

    /// Handle a parsed WithdrawEvent: save to DB, invalidate caches.
    async fn handle_withdraw(&self, signature: &str, evt: WithdrawEvent) -> Result<()> {
        if self.db.is_nullifier_used(&evt.nullifier_hash).await? {
            debug!("nullifier already indexed, skipping");
            return Ok(());
        }

        let recipient_b58 = bs58::encode(&evt.recipient).into_string();

        self.db
            .save_nullifier(
                &evt.nullifier_hash,
                &self.config.pool_address,
                Some(&recipient_b58),
                Some(evt.amount as i64),
                Some(signature),
            )
            .await?;

        {
            let mut cache = self.cache.lock().await;
            cache.invalidate_pool(&self.config.pool_address).await?;
        }

        metrics::record_withdrawal();
        info!(
            "withdraw indexed: recipient={}, amount={}",
            recipient_b58, evt.amount
        );

        Ok(())
    }
}

// ============================================================
// Event parsing
// ============================================================

/// Parse a DepositEvent payload (80 bytes, discriminator already stripped).
pub fn parse_deposit_event(payload: &[u8]) -> Result<DepositEvent> {
    if payload.len() < 80 {
        return Err(anyhow!("DepositEvent payload too short: {}", payload.len()));
    }
    let mut commitment = [0u8; 32];
    commitment.copy_from_slice(&payload[0..32]);

    let leaf_index = u64::from_le_bytes(payload[32..40].try_into().unwrap());

    let mut new_root = [0u8; 32];
    new_root.copy_from_slice(&payload[40..72]);

    let timestamp = i64::from_le_bytes(payload[72..80].try_into().unwrap());

    Ok(DepositEvent {
        commitment,
        leaf_index,
        new_root,
        timestamp,
    })
}

/// Parse a WithdrawEvent payload (80 bytes, discriminator already stripped).
pub fn parse_withdraw_event(payload: &[u8]) -> Result<WithdrawEvent> {
    if payload.len() < 80 {
        return Err(anyhow!(
            "WithdrawEvent payload too short: {}",
            payload.len()
        ));
    }
    let mut nullifier_hash = [0u8; 32];
    nullifier_hash.copy_from_slice(&payload[0..32]);

    let mut recipient = [0u8; 32];
    recipient.copy_from_slice(&payload[32..64]);

    let amount = u64::from_le_bytes(payload[64..72].try_into().unwrap());
    let timestamp = i64::from_le_bytes(payload[72..80].try_into().unwrap());

    Ok(WithdrawEvent {
        nullifier_hash,
        recipient,
        amount,
        timestamp,
    })
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_deposit_event() {
        let mut payload = vec![0u8; 80];
        payload[0..32].copy_from_slice(&[0x11u8; 32]);
        payload[32..40].copy_from_slice(&42u64.to_le_bytes());
        payload[40..72].copy_from_slice(&[0x22u8; 32]);
        payload[72..80].copy_from_slice(&1_700_000_000i64.to_le_bytes());

        let evt = parse_deposit_event(&payload).expect("parse");
        assert_eq!(evt.commitment, [0x11u8; 32]);
        assert_eq!(evt.leaf_index, 42);
        assert_eq!(evt.new_root, [0x22u8; 32]);
        assert_eq!(evt.timestamp, 1_700_000_000);
    }

    #[test]
    fn test_parse_withdraw_event() {
        let mut payload = vec![0u8; 80];
        payload[0..32].copy_from_slice(&[0x33u8; 32]);
        payload[32..64].copy_from_slice(&[0x44u8; 32]);
        payload[64..72].copy_from_slice(&1_000_000u64.to_le_bytes());
        payload[72..80].copy_from_slice(&1_700_000_000i64.to_le_bytes());

        let evt = parse_withdraw_event(&payload).expect("parse");
        assert_eq!(evt.nullifier_hash, [0x33u8; 32]);
        assert_eq!(evt.recipient, [0x44u8; 32]);
        assert_eq!(evt.amount, 1_000_000);
        assert_eq!(evt.timestamp, 1_700_000_000);
    }

    #[test]
    fn test_parse_deposit_too_short() {
        let payload = vec![0u8; 79];
        assert!(parse_deposit_event(&payload).is_err());
    }

    #[test]
    fn test_parse_withdraw_too_short() {
        let payload = vec![0u8; 79];
        assert!(parse_withdraw_event(&payload).is_err());
    }
}
