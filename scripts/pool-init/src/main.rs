//! pool-init — initialize the zkpool on-chain pool on Solana devnet.
//!
//! Builds the `pool` instruction by hand, signs it manually, and sends via
//! raw JSON-RPC. Avoids `solana-client` and the `wincode` feature (which has
//! a version conflict in the dependency tree).

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

fn default_program() -> String {
    dotenvy::var("PROGRAM_ID").expect("PROGRAM_ID must be set in .env")
}

#[derive(Debug)]
struct Args {
    rpc: String,
    keypair: PathBuf,
    program: Pubkey,
}

impl Args {
    fn from_env_and_argv() -> Result<Self> {
        let mut rpc = zkpool_common::default_rpc();
        let mut keypair = dirs_home()
            .context("cannot determine home directory")?
            .join(".config/solana/id.json");
        let mut program = Pubkey::from_str(&default_program())?;

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--rpc" => rpc = args.next().context("--rpc requires a value")?,
                "--keypair" => {
                    keypair = PathBuf::from(args.next().context("--keypair requires a value")?)
                }
                "--program" => {
                    program = Pubkey::from_str(&args.next().context("--program requires a value")?)?
                }
                "-h" | "--help" => {
                    println!(
                        "Usage: pool-init [--rpc <url>] [--keypair <path>] [--program <pubkey>]"
                    );
                    std::process::exit(0);
                }
                other => bail!("unknown argument: {}", other),
            }
        }

        Ok(Self {
            rpc,
            keypair,
            program,
        })
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args = Args::from_env_and_argv()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    println!("RPC:      {}", args.rpc);
    println!("Keypair:  {}", args.keypair.display());
    println!("Program:  {}", args.program);

    let keypair = load_keypair(&args.keypair)?;
    let authority = keypair.pubkey();
    println!("Authority: {}", authority);

    let pool_seed = zkpool_common::pool_seed();
    let vault_seed = zkpool_common::vault_seed();

    // Derive PDAs.
    let (pool_pda, pool_bump) =
        Pubkey::derive_program_address(&[&pool_seed], &args.program).context("derive pool PDA")?;
    let (vault_pda, vault_bump) =
        Pubkey::derive_program_address(&[&vault_seed, pool_pda.as_ref()], &args.program)
            .context("derive vault PDA")?;
    println!("Pool PDA:  {} (bump {})", pool_pda, pool_bump);
    println!("Vault PDA: {} (bump {})", vault_pda, vault_bump);

    if account_exists(&client, &args.rpc, &pool_pda).await? {
        println!("\n⚠️  Pool PDA already exists — nothing to do.");
        print_pool_state(&client, &args.rpc, &pool_pda).await?;
        return Ok(());
    }

    // Build instruction.
    let system_program = Pubkey::from_str(&zkpool_common::system_program_id())?;
    let ix = Instruction {
        program_id: args.program,
        accounts: vec![
            AccountMeta::new(authority, true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program, false),
        ],
        data: zkpool_common::pool_discriminator().to_vec(),
    };

    let blockhash = get_latest_blockhash(&client, &args.rpc).await?;
    println!("\nBlockhash: {}", blockhash);

    let message = Message::new_with_blockhash(&[ix], Some(&authority), &blockhash);
    let message_bytes = serialize_legacy_message(&message, &blockhash);

    let signature = keypair.sign_message(&message_bytes);

    let mut tx_bytes = Vec::with_capacity(1 + 64 + message_bytes.len());
    tx_bytes.push(1u8);
    tx_bytes.extend_from_slice(signature.as_ref());
    tx_bytes.extend_from_slice(&message_bytes);

    println!("Sending transaction ({} bytes)...", tx_bytes.len());

    let sig = send_transaction(&client, &args.rpc, &tx_bytes).await?;
    println!("Signature: {}", sig);

    println!("Waiting for confirmation...");
    let confirmed = wait_for_confirmation(&client, &args.rpc, &sig).await?;
    if !confirmed {
        bail!("transaction not confirmed within timeout");
    }
    println!("✅ Confirmed");

    print_pool_state(&client, &args.rpc, &pool_pda).await?;

    Ok(())
}

fn load_keypair(path: &PathBuf) -> Result<Keypair> {
    let data =
        std::fs::read(path).with_context(|| format!("reading keypair from {}", path.display()))?;
    let bytes: Vec<u8> = serde_json::from_slice(&data)
        .with_context(|| format!("parsing keypair JSON from {}", path.display()))?;
    Keypair::try_from(bytes.as_slice()).context("invalid keypair bytes")
}

async fn account_exists(client: &reqwest::Client, rpc: &str, pubkey: &Pubkey) -> Result<bool> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getAccountInfo",
        "params": [pubkey.to_string(), {"encoding": "base64"}],
    });
    let resp: Value = client.post(rpc).json(&body).send().await?.json().await?;
    let value = resp.pointer("/result/value");
    Ok(matches!(value, Some(v) if !v.is_null()))
}

async fn get_latest_blockhash(client: &reqwest::Client, rpc: &str) -> Result<Hash> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestBlockhash",
        "params": [{"commitment": "confirmed"}],
    });
    let resp: Value = client.post(rpc).json(&body).send().await?.json().await?;
    let bh_str = resp
        .pointer("/result/value/blockhash")
        .and_then(|v| v.as_str())
        .context("getLatestBlockhash response missing blockhash")?;
    Ok(Hash::from_str(bh_str)?)
}

async fn send_transaction(client: &reqwest::Client, rpc: &str, tx_bytes: &[u8]) -> Result<String> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(tx_bytes);
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [b64, {"encoding": "base64", "skipPreflight": false}],
    });
    let resp: Value = client.post(rpc).json(&body).send().await?.json().await?;
    if let Some(err) = resp.get("error") {
        bail!("sendTransaction RPC error: {}", err);
    }
    resp.pointer("/result")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .context("sendTransaction response missing signature")
}

async fn wait_for_confirmation(client: &reqwest::Client, rpc: &str, sig: &str) -> Result<bool> {
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [[sig], {"searchTransactionHistory": true}],
        });
        let resp: Value = client.post(rpc).json(&body).send().await?.json().await?;
        let status = resp.pointer("/result/value/0");
        if let Some(s) = status {
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

async fn print_pool_state(client: &reqwest::Client, rpc: &str, pubkey: &Pubkey) -> Result<()> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getAccountInfo",
        "params": [pubkey.to_string(), {"encoding": "base64"}],
    });
    let resp: Value = client.post(rpc).json(&body).send().await?.json().await?;
    let data_b64 = resp
        .pointer("/result/value/data/0")
        .and_then(|v| v.as_str());
    match data_b64 {
        Some(b64) => {
            use base64::Engine;
            let data = base64::engine::general_purpose::STANDARD.decode(b64)?;
            println!("\n✅ Pool account exists ({} bytes)", data.len());
            if data.len() >= 8 + 32 + 8 + 8 + 1 {
                let authority = Pubkey::try_from(&data[8..40]).context("authority")?;
                let next_leaf_index = u64::from_le_bytes(data[40..48].try_into().unwrap());
                let total_deposits = u64::from_le_bytes(data[48..56].try_into().unwrap());
                let current_root_index = data[56];
                println!("   authority:          {}", authority);
                println!("   next_leaf_index:    {}", next_leaf_index);
                println!("   total_deposits:     {}", total_deposits);
                println!("   current_root_index: {}", current_root_index);
            }
        }
        None => {
            println!("\n⚠️  Pool account does not exist");
        }
    }
    Ok(())
}

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
