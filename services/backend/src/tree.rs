//! Incremental Merkle tree.
//!
//! ## Design
//!
//! The tree has 2^DEPTH leaves (DEPTH = 20 for zkpool-solana). Keeping the
//! full tree in Redis is impractical (1M hashes); instead we keep only the
//! **rightmost non-empty node at each level**, plus the number of leaves.
//!
//! When we add a new leaf at index `n` (0-based):
//!   1. The new leaf becomes the current node at level 0.
//!   2. For each level `d` from 0 to DEPTH-1:
//!      - If `(n >> d) & 1 == 0` (our node is the LEFT child):
//!          sibling = stored rightmost node at level d
//!          parent = hash_2(current, sibling)
//!      - If `(n >> d) & 1 == 1` (our node is the RIGHT child):
//!          The left sibling was already merged into `current` in a
//!          previous iteration of the same `add_leaf` call (because our
//!          node is the rightmost, the left sibling is the last stored
//!          node). We use the stored rightmost as the left sibling.
//!          parent = hash_2(sibling, current)
//!   3. Store parent as the new rightmost node at level d+1.
//!   4. Final parent is the new root.
//!
//! ## Hashing
//!
//! Poseidon2 is NOT reimplemented here — we call the Merkle service over HTTP.
//! Different implementations (Rust, JS, Noir) produce different hashes; the
//! only way to guarantee identity is to use the same ACIR as the circuit.
//!
//! ## Redis keys for a pool P
//!
//!   tree:{P}:level:{d}   rightmost non-empty node at level d (hex, no 0x)
//!   tree:{P}:empty:{d}   empty hash at level d (hex, no 0x)
//!   tree:{P}:root        current root (hex, no 0x)
//!   tree:{P}:size        number of leaves inserted (decimal)

use anyhow::{anyhow, Context, Result};

use crate::cache::Cache;

/// Maximum tree depth supported by the code. Must match `spec.json`
/// (`circuit.tree_depth`) and on-chain `constants::TREE_DEPTH`.
pub const MAX_DEPTH: u32 = 32;

/// Client for the Merkle service.
#[derive(Clone)]
pub struct MerkleClient {
    http: reqwest::Client,
    base_url: String,
}

impl MerkleClient {
    /// Create a new client for the given base URL (e.g. `http://localhost:4003`).
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }

    /// Compute `hash_2(left, right)` via the Merkle service.
    ///
    /// Both inputs are 32-byte hex strings without `0x`.
    pub async fn hash_2(&self, left: &str, right: &str) -> Result<String> {
        let url = format!("{}/hash", self.base_url);
        let resp: serde_json::Value = self
            .http
            .post(&url)
            .json(&serde_json::json!({ "left": left, "right": right }))
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?
            .error_for_status()
            .with_context(|| format!("POST {} returned error status", url))?
            .json()
            .await
            .with_context(|| format!("failed to parse JSON from {}", url))?;

        resp.get("hash")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("Merkle service response missing `hash` field"))
    }
}

/// Incremental Merkle tree backed by Redis + Merkle service.
pub struct MerkleTree {
    pool: String,
    depth: u32,
    cache: Cache,
    merkle: MerkleClient,
}

impl MerkleTree {
    /// Create a tree handle for a specific pool.
    pub fn new(
        pool: impl Into<String>,
        depth: u32,
        cache: Cache,
        merkle: MerkleClient,
    ) -> Result<Self> {
        if depth == 0 || depth > MAX_DEPTH {
            return Err(anyhow!(
                "invalid tree depth {}: must be in 1..={}",
                depth,
                MAX_DEPTH
            ));
        }
        Ok(Self {
            pool: pool.into(),
            depth,
            cache,
            merkle,
        })
    }

    /// Initialize the tree in Redis if not already initialized.
    ///
    /// Computes the empty hashes for all levels:
    ///   empty[0] = 0^32 (all-zero leaf)
    ///   empty[d+1] = hash_2(empty[d], empty[d])
    ///
    /// Also sets the initial root = empty[DEPTH] and size = 0.
    pub async fn init_empty(&mut self) -> Result<()> {
        if self.cache.get(&self.key_root()).await?.is_some() {
            return Ok(());
        }

        let mut prev_empty = "00".repeat(32); // 32 zero bytes in hex

        // empty[0]
        self.cache.set(&self.key_empty(0), &prev_empty).await?;
        // level 0 rightmost starts as empty[0]
        self.cache.set(&self.key_level(0), &prev_empty).await?;

        for d in 1..=self.depth {
            let h = self.merkle.hash_2(&prev_empty, &prev_empty).await?;
            self.cache.set(&self.key_empty(d), &h).await?;
            self.cache.set(&self.key_level(d), &h).await?;
            prev_empty = h;
        }

        self.cache.set(&self.key_root(), &prev_empty).await?;
        self.cache.set(&self.key_size(), "0").await?;

        Ok(())
    }

    /// Add a new leaf, update the tree, return the new root.
    ///
    /// `leaf_hex` is a 32-byte hash as a hex string without `0x`.
    pub async fn add_leaf(&mut self, leaf_hex: &str) -> Result<String> {
        if leaf_hex.len() != 64 {
            return Err(anyhow!(
                "leaf_hex must be 64 hex chars (32 bytes), got {}",
                leaf_hex.len()
            ));
        }

        self.init_empty().await?;

        // Current size = index of the new leaf (0-based).
        let size: u64 = self
            .cache
            .get(&self.key_size())
            .await?
            .unwrap_or_else(|| "0".to_string())
            .parse()
            .context("failed to parse tree size")?;

        let mut current = leaf_hex.to_string();

        for d in 0..self.depth {
            // Read the current rightmost at this level (before we update it).
            let rightmost = self
                .cache
                .get(&self.key_level(d))
                .await?
                .unwrap_or_else(|| self.empty(d));

            // Determine if our node is the LEFT child at this level.
            let is_left = (size >> d) & 1 == 0;

            let (left, right) = if is_left {
                // Our node is left, sibling (rightmost) is right.
                (current.clone(), rightmost)
            } else {
                // Our node is right, the stored rightmost is the left sibling.
                (rightmost, current.clone())
            };

            let parent = self.merkle.hash_2(&left, &right).await?;

            // Update the rightmost at level d+1.
            self.cache.set(&self.key_level(d + 1), &parent).await?;

            current = parent;
        }

        let new_size = size + 1;
        self.cache
            .set(&self.key_size(), &new_size.to_string())
            .await?;
        self.cache.set(&self.key_root(), &current).await?;

        Ok(current)
    }

    /// Get the current root.
    pub async fn root(&mut self) -> Result<Option<String>> {
        self.cache.get(&self.key_root()).await
    }

    /// Get the number of leaves.
    pub async fn size(&mut self) -> Result<u64> {
        match self.cache.get(&self.key_size()).await? {
            Some(s) => s.parse().context("failed to parse tree size"),
            None => Ok(0),
        }
    }

    // ---------- Redis key helpers ----------

    fn key_level(&self, d: u32) -> String {
        format!("tree:{}:level:{}", self.pool, d)
    }

    fn key_empty(&self, d: u32) -> String {
        format!("tree:{}:empty:{}", self.pool, d)
    }

    fn key_root(&self) -> String {
        format!("tree:{}:root", self.pool)
    }

    fn key_size(&self) -> String {
        format!("tree:{}:size", self.pool)
    }

    /// Placeholder empty-hash fallback. The real value is stored in Redis;
    /// this is only used if a key is somehow missing (e.g. Redis was wiped
    /// mid-operation). Returns all-zero bytes as a safe default.
    fn empty(&self, _d: u32) -> String {
        "00".repeat(32)
    }
}
