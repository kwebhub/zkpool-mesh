//! Configuration loaded from environment variables.
//!
//! All required variables must be set; the process exits with a clear error
//! if any are missing. Optional variables have sensible defaults.
//!
//! See `.env.example` in the project root for the full list.

use anyhow::{Context, Result};

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Postgres connection URL.
    pub database_url: String,

    /// Redis connection URL.
    pub redis_url: String,

    /// HTTP server port.
    pub port: u16,

    /// Solana RPC URL (for the indexer).
    pub solana_rpc_url: String,

    /// Pool PDA address — the indexer filters events by this pool.
    pub pool_address: String,
    pub verifier_program_id: String,
    pub program_id: String,

    /// Merkle service URL (used by `tree.rs` for Poseidon2 hashing).
    pub merkle_url: String,

    /// Prover service URL (used by `/api/withdraw`).
    pub prover_url: String,

    /// Indexer polling interval (seconds).
    pub indexer_poll_interval_secs: u64,

    /// Max signatures per page when polling.
    pub indexer_page_size: usize,

    /// Max pages of signatures to fetch per poll cycle.
    pub indexer_max_pages: usize,

    /// Rate limit for `/api/withdraw` per minute, per IP.
    pub rate_limit_withdraw_per_min: u64,

    /// Rate limit for read endpoints per minute, per IP.
    pub rate_limit_read_per_min: u64,

    /// Merkle tree depth (must match spec and on-chain constant).
    pub merkle_tree_depth: u32,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// `dotenvy::dotenv()` is called first, so a `.env` file in the current
    /// working directory is loaded (if present).
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        Ok(Self {
            database_url: required("DATABASE_URL")?,
            redis_url: required("REDIS_URL")?,
            port: optional_parse("PORT", 4001)?,
            solana_rpc_url: required("SOLANA_RPC_URL")?,
            pool_address: required("POOL_ADDRESS")?,
            verifier_program_id: required("VERIFIER_PROGRAM_ID")?,
            program_id: required("PROGRAM_ID")?,
            merkle_url: optional("MERKLE_URL", "http://localhost:4003"),
            prover_url: optional("PROVER_URL", "http://localhost:4002"),
            indexer_poll_interval_secs: optional_parse("INDEXER_POLL_INTERVAL_SECS", 5)?,
            indexer_page_size: optional_parse("INDEXER_PAGE_SIZE", 100)?,
            indexer_max_pages: optional_parse("INDEXER_MAX_PAGES", 10)?,
            rate_limit_withdraw_per_min: optional_parse("RATE_LIMIT_WITHDRAW_PER_MIN", 5)?,
            rate_limit_read_per_min: optional_parse("RATE_LIMIT_READ_PER_MIN", 60)?,
            merkle_tree_depth: optional_parse("MERKLE_TREE_DEPTH", 20)?,
        })
    }
}

/// Reads a required env var, returns an error if missing.
fn required(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("missing required env var: {}", name))
}

/// Reads an optional env var with a default.
fn optional(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Reads an optional env var, parses it, or uses default.
fn optional_parse<T>(name: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match std::env::var(name) {
        Ok(v) => v
            .parse::<T>()
            .with_context(|| format!("invalid value for {}: {:?}", name, v)),
        Err(_) => Ok(default),
    }
}
