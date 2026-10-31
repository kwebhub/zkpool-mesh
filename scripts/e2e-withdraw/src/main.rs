//! e2e-withdraw — end-to-end withdrawal script.
//!
//! Stage 15.9: supports the split-deposit note format (`splits`, `note_index`)
//! and the 6-public-input withdrawal circuit.
//!
//! Pipeline:
//!   1. Read note JSON from a file (path via argv[1]).
//!   2. Reduce recipient → BN254 field element.
//!   3. Compute recipient_binding via Merkle POST /hash.
//!   4. Look up leaf_index in /api/commitments.
//!   5. Fetch Merkle proof via /api/proof.
//!   6. Fetch root via /api/root.
//!   7. POST /api/withdraw → Groth16 proof + public witness (base64).
//!   8. Build + sign + send `withdraw` instruction.
//!
//! Usage: e2e-withdraw <note.json> <recipient_base58>

use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use solana_hash::Hash;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use zk_pool::encoding::reduce_to_field;

/// SetComputeUnitLimit instruction discriminator.
const SET_COMPUTE_UNIT_LIMIT_IX: u8 = 2;

/// Groth16 proof length in bytes (Stage 15.4 — was 324).
const PROOF_LEN: usize = 388;
/// Number of splits per deposit.
const SPLIT_COUNT: usize = 3;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let note_path = std::env::args()
        .nth(1)
        .context("usage: e2e-withdraw <note.json> <recipient_base58>")?;
    let recipient_str = std::env::args().nth(2).context("missing recipient")?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    // ---- 1. Load keypair + note ----
    let home = std::env::var("HOME")?;
    let keypair_path = PathBuf::from(&home).join(".config/solana/id.json");
    let payer = load_keypair(&keypair_path)?;
    println!("Payer:        {}", payer.pubkey());

    let note_text = std::fs::read_to_string(&note_path)
        .with_context(|| format!("reading note from {}", note_path))?;
    let note: Value = serde_json::from_str(&note_text).context("parsing note JSON")?;

    let nullifier = note["nullifier"].as_str().context("missing nullifier")?;
    let secret = note["secret"].as_str().context("missing secret")?;
    let note_secret = note["note_secret"]
        .as_str()
        .context("missing note_secret")?;
    let amount_hex = note["amount"].as_str().context("missing amount")?;
    let commitment = note["commitment"].as_str().context("missing commitment")?;
    let splits: Vec<String> = note["splits"]
        .as_array()
        .context("missing splits")?
        .iter()
        .map(|v| v.as_str().unwrap_or("").to_string())
        .collect();
    let note_index: u64 = note["note_index"].as_u64().context("missing note_index")?;

    if splits.len() != SPLIT_COUNT {
        bail!(
            "splits must have {} elements, got {}",
            SPLIT_COUNT,
            splits.len()
        );
    }
    if note_index as usize >= SPLIT_COUNT {
        bail!("note_index must be < {}, got {}", SPLIT_COUNT, note_index);
    }

    // total_amount = Σ splits[i]
    let total_amount_u64: u64 = splits
        .iter()
        .map(|s| u64::from_str_radix(s, 16))
        .collect::<Result<Vec<u64>, _>>()?
        .iter()
        .sum();
    let total_amount_hex = format!("{:064x}", total_amount_u64);

    println!("commitment:   {}", commitment);
    println!(
        "amount:       {} (0x{})",
        u64::from_str_radix(amount_hex, 16)?,
        amount_hex
    );
    println!(
        "total_amount: {} (0x{})",
        total_amount_u64, total_amount_hex
    );
    println!("note_index:   {}", note_index);

    // ---- 2. Recipient reduction ----
    let recipient_pubkey = Pubkey::from_str(&recipient_str)?;
    let recipient_bytes: [u8; 32] = recipient_pubkey.to_bytes();
    let recipient_reduced = reduce_to_field(&recipient_bytes);
    let recipient_field_hex = hex::encode(recipient_reduced);
    println!("recipient:    {}", recipient_pubkey);
    println!("recpt(field): {}", recipient_field_hex);

    // ---- 3. Compute recipient_binding via Merkle /hash ----
    let hash_body = json!({
        "left": note_secret,
        "right": recipient_field_hex,
    });
    let hash_resp: Value = client
        .post(format!("{}/hash", &zkpool_common::merkle_url()))
        .json(&hash_body)
        .send()
        .await
        .context("POST /hash failed")?
        .error_for_status()
        .context("POST /hash returned error")?
        .json()
        .await?;
    let recipient_binding = hash_resp["hash"]
        .as_str()
        .context("missing hash")?
        .to_string();
    println!("recpt_bind:   {}", recipient_binding);

    // ---- 4. Find leaf_index ----
    let commitments_resp: Value = client
        .get(format!("{}/api/commitments", &zkpool_common::backend_url()))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let leaf_index = commitments_resp["commitments"]
        .as_array()
        .context("commitments not array")?
        .iter()
        .find(|c| c["commitment"].as_str() == Some(commitment))
        .and_then(|c| c["leaf_index"].as_u64())
        .context("commitment not found in /api/commitments")?;
    println!("leaf_index:   {}", leaf_index);

    // ---- 5. Merkle proof ----
    let proof_resp: Value = client
        .get(format!(
            "{}/api/proof?leaf_index={}",
            &zkpool_common::backend_url(), leaf_index
        ))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let merkle_proof: Vec<String> = proof_resp["proof"]
        .as_array()
        .context("proof not array")?
        .iter()
        .map(|v| v.as_str().unwrap_or("").to_string())
        .collect();
    let is_even: Vec<bool> = proof_resp["is_even"]
        .as_array()
        .context("is_even not array")?
        .iter()
        .map(|v| v.as_bool().unwrap_or(false))
        .collect();
    println!(
        "merkle_proof: {} items, is_even: {} items",
        merkle_proof.len(),
        is_even.len()
    );

    // ---- 6. Root ----
    let root_resp: Value = client
        .get(format!("{}/api/root", &zkpool_common::backend_url()))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let root = root_resp["root"]
        .as_str()
        .context("missing root")?
        .to_string();
    println!("root:         {}", root);

    // ---- 7. Groth16 proof via /api/withdraw ----
    let withdraw_body = json!({
        "root": root,
        "nullifier_hash": nullifier_hash_from_note(&note)?,
        "recipient": recipient_field_hex,
        "recipient_binding": recipient_binding,
        "amount": amount_hex,
        "total_amount": total_amount_hex,
        "nullifier": nullifier,
        "secret": secret,
        "note_secret": note_secret,
        "merkle_proof": merkle_proof,
        "is_even": is_even,
        "splits": splits,
        "note_index": note_index,
    });
    let withdraw_resp: Value = client
        .post(format!("{}/api/withdraw", &zkpool_common::backend_url()))
        .json(&withdraw_body)
        .send()
        .await
        .context("POST /api/withdraw failed")?
        .error_for_status()
        .context("POST /api/withdraw returned error")?
        .json()
        .await?;

    let proof_b64 = withdraw_resp["proof"].as_str().context("missing proof")?;
    let pw_b64 = withdraw_resp["public_witness"]
        .as_str()
        .context("missing public_witness")?;

    use base64::Engine;
    let proof_bytes = base64::engine::general_purpose::STANDARD.decode(proof_b64)?;
    let _pw_bytes = base64::engine::general_purpose::STANDARD.decode(pw_b64)?;
    println!("proof:        {} bytes", proof_bytes.len());
    if proof_bytes.len() != PROOF_LEN {
        bail!(
            "unexpected proof length: {}, expected {}",
            proof_bytes.len(),
            PROOF_LEN
        );
    }

    // ---- 8. Build the withdraw instruction ----
    let program_id = Pubkey::from_str(&zkpool_common::program_id())?;
    let verifier_id = Pubkey::from_str(&zkpool_common::verifier_program_id())?;
    let (pool_pda, _) = Pubkey::derive_program_address(
        &[&zkpool_common::pool_seed()],
        &program_id,
    )
    .context("pool PDA")?;
    let (vault_pda, _) = Pubkey::derive_program_address(
        &[&zkpool_common::vault_seed(), pool_pda.as_ref()],
        &program_id,
    )
    .context("vault PDA")?;

    let nullifier_hash_bytes = hex::decode(nullifier_hash_from_note(&note)?)?;
    let nullifier_hash_arr: [u8; 32] = nullifier_hash_bytes
        .as_slice()
        .try_into()
        .context("nullifier_hash not 32 bytes")?;
    let (nullifier_record_pda, _) = Pubkey::derive_program_address(
        &[
            &zkpool_common::nullifier_record_seed(),
            pool_pda.as_ref(),
            &nullifier_hash_arr,
        ],
        &program_id,
    )
    .context("nullifier record PDA")?;
    println!("null_rec PDA: {}", nullifier_record_pda);

    let root_bytes = hex::decode(&root)?;
    let nullifier_hash_bytes = hex::decode(nullifier_hash_from_note(&note)?)?;
    let recipient_binding_bytes = hex::decode(&recipient_binding)?;
    let total_amount_bytes = hex::decode(&total_amount_hex)?;
    let amount_u64: u64 = u64::from_str_radix(amount_hex, 16)?;

    // Data layout (Anchor Borsh), Stage 15.5:
    //   [disc(8)]
    //   || proof: Vec<u8> = u32 LE length + bytes
    //   || nullifier_hash: [u8; 32]
    //   || root: [u8; 32]
    //   || recipient: Pubkey (32)
    //   || amount: u64 (LE)
    //   || recipient_binding: [u8; 32]
    //   || total_amount: u64 (LE)
    let mut ix_data = Vec::with_capacity(8 + 4 + PROOF_LEN + 32 + 32 + 32 + 8 + 32 + 8);
    ix_data.extend_from_slice(&zkpool_common::withdraw_discriminator());
    ix_data.extend_from_slice(&(proof_bytes.len() as u32).to_le_bytes());
    ix_data.extend_from_slice(&proof_bytes);
    ix_data.extend_from_slice(&nullifier_hash_bytes);
    ix_data.extend_from_slice(&root_bytes);
    ix_data.extend_from_slice(&recipient_pubkey.to_bytes());
    ix_data.extend_from_slice(&amount_u64.to_le_bytes());
    ix_data.extend_from_slice(&recipient_binding_bytes);
    ix_data.extend_from_slice(&total_amount_u64.to_le_bytes());
    let _ = total_amount_bytes; // not needed in the wire format

    let system_program = Pubkey::from_str(&zkpool_common::system_program_id())?;
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer.pubkey(), true),           // payer
            AccountMeta::new(pool_pda, false),                // pool
            AccountMeta::new(nullifier_record_pda, false),    // nullifier_record
            AccountMeta::new(vault_pda, false),               // vault
            AccountMeta::new(recipient_pubkey, false),        // to
            AccountMeta::new_readonly(verifier_id, false),    // verifier_program
            AccountMeta::new_readonly(system_program, false), // system_program
        ],
        data: ix_data,
    };

    // ---- 9. Send ----
    let compute_budget_id = Pubkey::from_str(&zkpool_common::compute_budget_program_id())?;
    let mut cb_data = Vec::with_capacity(5);
    cb_data.push(SET_COMPUTE_UNIT_LIMIT_IX);
    cb_data.extend_from_slice(&1_400_000u32.to_le_bytes());
    let compute_budget_ix = Instruction {
        program_id: compute_budget_id,
        accounts: vec![],
        data: cb_data,
    };

    let blockhash = get_latest_blockhash(&client).await?;
    let message =
        Message::new_with_blockhash(&[compute_budget_ix, ix], Some(&payer.pubkey()), &blockhash);
    let message_bytes = serialize_legacy_message(&message, &blockhash);
    let signature = payer.sign_message(&message_bytes);

    let mut tx_bytes = Vec::with_capacity(1 + 64 + message_bytes.len());
    tx_bytes.push(1u8);
    tx_bytes.extend_from_slice(signature.as_ref());
    tx_bytes.extend_from_slice(&message_bytes);

    let sig = send_transaction(&client, &tx_bytes).await?;
    println!("signature:    {}", sig);

    println!("confirming...");
    if !wait_for_confirmation(&client, &sig).await? {
        bail!("transaction not confirmed");
    }
    println!("✅ Withdrawal complete");

    Ok(())
}

fn nullifier_hash_from_note(note: &Value) -> Result<String> {
    note["nullifier_hash"]
        .as_str()
        .map(|s| s.to_string())
        .context("missing nullifier_hash in note")
}

fn load_keypair(path: &PathBuf) -> Result<Keypair> {
    let data = std::fs::read(path)?;
    let bytes: Vec<u8> = serde_json::from_slice(&data)?;
    Keypair::try_from(bytes.as_slice()).context("invalid keypair")
}

async fn get_latest_blockhash(client: &reqwest::Client) -> Result<Hash> {
    let body = json!({"jsonrpc":"2.0","id":1,"method":"getLatestBlockhash","params":[{"commitment":"confirmed"}]});
    let resp: Value = client
        .post(&zkpool_common::rpc_url())
        .json(&body)
        .send()
        .await?
        .json()
        .await?;
    Ok(Hash::from_str(
        resp.pointer("/result/value/blockhash")
            .and_then(|v| v.as_str())
            .context("blockhash")?,
    )?)
}

async fn send_transaction(client: &reqwest::Client, tx_bytes: &[u8]) -> Result<String> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(tx_bytes);
    let body = json!({"jsonrpc":"2.0","id":1,"method":"sendTransaction","params":[b64,{"encoding":"base64","skipPreflight":false}]});
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
    Ok(resp
        .pointer("/result")
        .and_then(|v| v.as_str())
        .context("signature")?
        .to_string())
}

async fn wait_for_confirmation(client: &reqwest::Client, sig: &str) -> Result<bool> {
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let body = json!({"jsonrpc":"2.0","id":1,"method":"getSignatureStatuses","params":[[sig],{"searchTransactionHistory":true}]});
        let resp: Value = client
            .post(&zkpool_common::rpc_url())
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        if let Some(s) = resp.pointer("/result/value/0") {
            if !s.is_null() {
                let c = s.get("confirmationStatus").and_then(|v| v.as_str());
                if matches!(c, Some("confirmed") | Some("finalized")) {
                    return Ok(true);
                }
                if let Some(err) = s.get("err") {
                    if !err.is_null() {
                        bail!("tx failed: {}", err);
                    }
                }
            }
        }
    }
    Ok(false)
}

fn serialize_legacy_message(msg: &Message, blockhash: &Hash) -> Vec<u8> {
    fn write_compact_u16(buf: &mut Vec<u8>, mut n: u16) {
        loop {
            let mut b = (n & 0x7f) as u8;
            n >>= 7;
            if n == 0 {
                buf.push(b);
                break;
            }
            b |= 0x80;
            buf.push(b);
        }
    }
    let mut buf = Vec::new();
    buf.push(msg.header.num_required_signatures);
    buf.push(msg.header.num_readonly_signed_accounts);
    buf.push(msg.header.num_readonly_unsigned_accounts);
    write_compact_u16(&mut buf, msg.account_keys.len() as u16);
    for k in &msg.account_keys {
        buf.extend_from_slice(k.as_ref());
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
