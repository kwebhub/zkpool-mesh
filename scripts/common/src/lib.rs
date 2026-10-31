use anyhow::Context;

pub fn rpc_url() -> String {
    dotenvy::var("RPC_URL").context("RPC_URL must be set in .env").unwrap()
}

pub fn backend_url() -> String {
    dotenvy::var("BACKEND_URL").context("BACKEND_URL must be set in .env").unwrap()
}

pub fn merkle_url() -> String {
    dotenvy::var("MERKLE_URL").context("MERKLE_URL must be set in .env").unwrap()
}

pub fn system_program_id() -> String {
    dotenvy::var("SYSTEM_PROGRAM_ID").context("SYSTEM_PROGRAM_ID must be set in .env").unwrap()
}

pub fn pool_seed() -> Vec<u8> {
    dotenvy::var("POOL_SEED")
        .context("POOL_SEED must be set in .env")
        .unwrap()
        .into_bytes()
}

pub fn vault_seed() -> Vec<u8> {
    dotenvy::var("VAULT_SEED")
        .context("VAULT_SEED must be set in .env")
        .unwrap()
        .into_bytes()
}

pub fn nullifier_record_seed() -> Vec<u8> {
    dotenvy::var("NULLIFIER_RECORD_SEED")
        .context("NULLIFIER_RECORD_SEED must be set in .env")
        .unwrap()
        .into_bytes()
}

pub fn deposit_discriminator() -> [u8; 8] {
    parse_discriminator("DEPOSIT_DISCRIMINATOR")
}

pub fn deposit_split_discriminator() -> [u8; 8] {
    parse_discriminator("DEPOSIT_SPLIT_DISCRIMINATOR")
}

pub fn withdraw_discriminator() -> [u8; 8] {
    parse_discriminator("WITHDRAW_DISCRIMINATOR")
}

pub fn compute_budget_program_id() -> String {
    dotenvy::var("COMPUTE_BUDGET_PROGRAM_ID")
        .context("COMPUTE_BUDGET_PROGRAM_ID must be set in .env")
        .unwrap()
}

fn parse_discriminator(key: &str) -> [u8; 8] {
    let raw = dotenvy::var(key)
        .with_context(|| format!("{} must be set in .env", key))
        .unwrap();
    let bytes: Vec<u8> = raw
        .split(',')
        .map(|s| s.trim().parse::<u8>().expect("invalid discriminator byte"))
        .collect();
    assert_eq!(bytes.len(), 8, "{} must have 8 bytes", key);
    let mut out = [0u8; 8];
    out.copy_from_slice(&bytes);
    out
}

pub fn program_id() -> String {
    dotenvy::var("PROGRAM_ID").context("PROGRAM_ID must be set in .env").unwrap()
}

pub fn verifier_program_id() -> String {
    dotenvy::var("VERIFIER_PROGRAM_ID").context("VERIFIER_PROGRAM_ID must be set in .env").unwrap()
}

pub fn pool_address() -> String {
    dotenvy::var("POOL_ADDRESS").context("POOL_ADDRESS must be set in .env").unwrap()
}

pub fn pool_discriminator() -> [u8; 8] {
    parse_discriminator("POOL_DISCRIMINATOR")
}

pub fn default_rpc() -> String {
    dotenvy::var("DEFAULT_RPC").context("DEFAULT_RPC must be set in .env").unwrap()
}

pub fn split_amounts() -> Vec<u64> {
    let raw = dotenvy::var("SPLIT_AMOUNTS").context("SPLIT_AMOUNTS must be set in .env").unwrap();
    raw.split(',')
        .map(|s| s.trim().parse::<u64>().expect("invalid split amount"))
        .collect()
}
