//! e2e-deposit — end-to-end deposit script.
//!
//! Simulates the browser UI's deposit flow in Rust:
//!   1. Generate a fresh note (nullifier, secret, note_secret).
//!   2. Call Merkle `POST /hashes` → commitment + nullifier_hash.
//!   3. GET /api/commitments.
//!   4. POST /api/root-preview → new_root.
//!   5. Build + sign + send `deposit` instruction.
//!   6. Print the note for withdrawal.
//!
//! Usage: e2e-deposit [amount_lamports]
//! Default: 1_000_000 lamports (0.001 SOL).

use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use rand::RngCore;
use serde_json::{json, Value};
use solana_hash::Hash;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_signer::Signer;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let amount_lamports: u64 = std::env::args()
        .nth(1)
        .map(|s| s.parse())
        .transpose()
        .context("invalid amount")?
        .unwrap_or(1_000_000);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    // ---- 1. Load keypair ----
    let home = std::env::var("HOME").context("HOME not set")?;
    let keypair_path = PathBuf::from(&home).join(".config/solana/id.json");
    let keypair = load_keypair(&keypair_path)?;
    println!("Wallet:       {}", keypair.pubkey());

    // ---- 2. Generate random note ----
    let nullifier = random_field_hex();
    let secret = random_field_hex();
    let note_secret = random_field_hex();
    let amount_hex = format!("{:064x}", amount_lamports);

    println!("nullifier:    {}", nullifier);
    println!("secret:       {}", secret);
    println!("note_secret:  {}", note_secret);

    // ---- 3. Compute hashes via Merkle /hashes ----
    let hashes_body = json!({
        "nullifier": nullifier,
        "secret": secret,
        "amount": amount_hex,
    });
    let hashes_resp: Value = client
        .post(format!("{}/hashes", &zkpool_common::merkle_url()))
        .json(&hashes_body)
        .send()
        .await
        .context("POST /hashes failed")?
        .error_for_status()
        .context("POST /hashes returned error")?
        .json()
        .await?;

    let commitment = hashes_resp["commitment"]
        .as_str()
        .context("missing commitment")?
        .to_string();
    let nullifier_hash = hashes_resp["nullifier_hash"]
        .as_str()
        .context("missing nullifier_hash")?
        .to_string();

    println!("commitment:   {}", commitment);
    println!("null_hash:    {}", nullifier_hash);

    // ---- 4. Fetch commitments + preview root ----
    let commitments_resp: Value = client
        .get(format!("{}/api/commitments", &zkpool_common::backend_url()))
        .send()
        .await
        .context("GET /api/commitments failed")?
        .error_for_status()?
        .json()
        .await?;

    let mut commitment_list: Vec<String> = commitments_resp["commitments"]
        .as_array()
        .context("commitments not array")?
        .iter()
        .filter_map(|c| c["commitment"].as_str().map(|s| s.to_string()))
        .collect();
    commitment_list.push(commitment.clone());

    let root_body = json!({ "commitments": commitment_list });
    let root_resp: Value = client
        .post(format!("{}/api/root-preview", &zkpool_common::backend_url()))
        .json(&root_body)
        .send()
        .await
        .context("POST /api/root-preview failed")?
        .error_for_status()?
        .json()
        .await?;

    let new_root = root_resp["root"]
        .as_str()
        .context("missing root")?
        .to_string();
    println!("new_root:     {}", new_root);

    // ---- 5. Build the deposit instruction ----
    let program_id = Pubkey::from_str(&zkpool_common::program_id())?;
    let (pool_pda, _) =
        Pubkey::derive_program_address(&[&zkpool_common::pool_seed()], &program_id).context("derive pool PDA")?;
    let (vault_pda, _) =
        Pubkey::derive_program_address(&[&zkpool_common::vault_seed(), pool_pda.as_ref()], &program_id)
            .context("derive vault PDA")?;
    println!("pool PDA:     {}", pool_pda);
    println!("vault PDA:    {}", vault_pda);

    let commitment_bytes = hex::decode(&commitment)?;
    let root_bytes = hex::decode(&new_root)?;
    if commitment_bytes.len() != 32 {
        bail!("commitment not 32 bytes");
    }
    if root_bytes.len() != 32 {
        bail!("root not 32 bytes");
    }

    let mut ix_data = Vec::with_capacity(8 + 32 + 32 + 8);
    ix_data.extend_from_slice(&zkpool_common::deposit_discriminator());
    ix_data.extend_from_slice(&commitment_bytes);
    ix_data.extend_from_slice(&root_bytes);
    ix_data.extend_from_slice(&amount_lamports.to_le_bytes());

    let system_program = Pubkey::from_str(&zkpool_common::system_program_id())?;
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(keypair.pubkey(), true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: ix_data,
    };

    // ---- 6. Get blockhash, build, sign, send ----
    let blockhash = get_latest_blockhash(&client).await?;
    println!("blockhash:    {}", blockhash);

    let message = Message::new_with_blockhash(&[ix], Some(&keypair.pubkey()), &blockhash);
    let message_bytes = serialize_legacy_message(&message, &blockhash);
    let signature = keypair.sign_message(&message_bytes);

    let mut tx_bytes = Vec::with_capacity(1 + 64 + message_bytes.len());
    tx_bytes.push(1u8);
    tx_bytes.extend_from_slice(signature.as_ref());
    tx_bytes.extend_from_slice(&message_bytes);

    let sig = send_transaction(&client, &tx_bytes).await?;
    println!("signature:    {}", sig);

    // ---- 7. Confirm ----
    println!("confirming...");
    if !wait_for_confirmation(&client, &sig).await? {
        bail!("transaction not confirmed");
    }
    println!("✅ Confirmed");

    // ---- 8. Print note ----
    println!("\n=== NOTE (save for withdrawal) ===");
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "nullifier": nullifier,
            "secret": secret,
            "note_secret": note_secret,
            "amount": amount_hex,
            "commitment": commitment,
            "nullifier_hash": nullifier_hash,
            "tx_signature": sig,
            "pool_pda": pool_pda.to_string(),
        }))?
    );

    Ok(())
}

fn random_field_hex() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    // Clear top 3 bits — the BN254 prime begins with 0x30, so keeping the
    // top byte ≤ 0x1f guarantees the value is below the modulus.
    bytes[0] &= 0x1f;
    hex::encode(bytes)
}

fn load_keypair(path: &PathBuf) -> Result<Keypair> {
    let data =
        std::fs::read(path).with_context(|| format!("reading keypair from {}", path.display()))?;
    let bytes: Vec<u8> = serde_json::from_slice(&data)?;
    Keypair::try_from(bytes.as_slice()).context("invalid keypair bytes")
}

async fn get_latest_blockhash(client: &reqwest::Client) -> Result<Hash> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestBlockhash",
        "params": [{"commitment": "confirmed"}],
    });
    let resp: Value = client
        .post(&zkpool_common::rpc_url())
        .json(&body)
        .send()
        .await?
        .json()
        .await?;
    let bh_str = resp
        .pointer("/result/value/blockhash")
        .and_then(|v| v.as_str())
        .context("missing blockhash")?;
    Ok(Hash::from_str(bh_str)?)
}

async fn send_transaction(client: &reqwest::Client, tx_bytes: &[u8]) -> Result<String> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(tx_bytes);
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [b64, {"encoding": "base64", "skipPreflight": false}],
    });
    let resp: Value = client
        .post(&zkpool_common::rpc_url())
        .json(&body)
        .send()
        .await?
        .json()
        .await?;
    if let Some(err) = resp.get("error") {
        bail!("sendTransaction RPC error: {}", err);
    }
    resp.pointer("/result")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .context("missing signature")
}

async fn wait_for_confirmation(client: &reqwest::Client, sig: &str) -> Result<bool> {
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [[sig], {"searchTransactionHistory": true}],
        });
        let resp: Value = client
            .post(&zkpool_common::rpc_url())
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        if let Some(s) = resp.pointer("/result/value/0") {
            if !s.is_null() {
                let confirmation = s.get("confirmationStatus").and_then(|v| v.as_str());
                if matches!(confirmation, Some("confirmed") | Some("finalized")) {
                    return Ok(true);
                }
                if let Some(err) = s.get("err") {
                    if !err.is_null() {
                        bail!("transaction failed: {}", err);
                    }
                }
            }
        }
    }
    Ok(false)
}

/// Manually serialize a legacy Message into wire format.
fn serialize_legacy_message(msg: &Message, blockhash: &Hash) -> Vec<u8> {
    fn write_compact_u16(buf: &mut Vec<u8>, mut n: u16) {
        loop {
            let mut byte = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                buf.push(byte);
                break;
            } else {
                byte |= 0x80;
                buf.push(byte);
            }
        }
    }

    let mut buf = Vec::new();
    buf.push(msg.header.num_required_signatures);
    buf.push(msg.header.num_readonly_signed_accounts);
    buf.push(msg.header.num_readonly_unsigned_accounts);

    write_compact_u16(&mut buf, msg.account_keys.len() as u16);
    for key in &msg.account_keys {
        buf.extend_from_slice(key.as_ref());
    }

    buf.extend_from_slice(blockhash.as_ref());

    write_compact_u16(&mut buf, msg.instructions.len() as u16);
    for ix in &msg.instructions {
        buf.push(ix.program_id_index);
        write_compact_u16(&mut buf, ix.accounts.len() as u16);
        buf.extend_from_slice(&ix.accounts);
        write_compact_u16(&mut buf, ix.data.len() as u16);
        buf.extend_from_slice(&ix.data);
    }

    buf
}
