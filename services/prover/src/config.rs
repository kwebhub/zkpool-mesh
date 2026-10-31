//! Configuration loaded from environment variables.
//!
//! All paths default to sensible values for the `solana` container.
//! See `.env.example` in the project root for the full list.

use anyhow::{Context, Result};

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP server port.
    pub port: u16,

    /// Directory containing the withdrawal circuit source + target artifacts.
    /// Must contain `Nargo.toml`, `src/`, `target/withdrawal.json`,
    /// `target/withdrawal.ccs`, `target/withdrawal.pk`.
    pub circuit_dir: String,

    /// Path to the `nargo` binary (or just "nargo" to rely on PATH).
    pub nargo_bin: String,

    /// Path to the `sunspot` binary (or just "sunspot" to rely on PATH).
    pub sunspot_bin: String,

    /// Timeout for `nargo execute` (seconds).
    pub nargo_timeout_secs: u64,

    /// Timeout for `sunspot prove` (seconds).
    pub sunspot_timeout_secs: u64,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// `dotenvy::dotenv()` is called first, so a `.env` file in the current
    /// working directory is loaded (if present).
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        Ok(Self {
            port: optional_parse("PROVER_PORT", 4002)?,
            circuit_dir: optional("CIRCUIT_DIR", "/home/ubuntu/circuits/withdrawal"),
            nargo_bin: optional("NARGO_BIN", "nargo"),
            sunspot_bin: optional("SUNSPOT_BIN", "sunspot"),
            nargo_timeout_secs: optional_parse("NARGO_TIMEOUT_SECS", 30)?,
            sunspot_timeout_secs: optional_parse("SUNSPOT_TIMEOUT_SECS", 30)?,
        })
    }

    /// Path to the compiled ACIR file.
    pub fn acir_path(&self) -> String {
        format!("{}/target/withdrawal.json", self.circuit_dir)
    }

    /// Path to the CCS file.
    pub fn ccs_path(&self) -> String {
        format!("{}/target/withdrawal.ccs", self.circuit_dir)
    }

    /// Path to the proving key.
    pub fn pk_path(&self) -> String {
        format!("{}/target/withdrawal.pk", self.circuit_dir)
    }

    /// Path to the proof file produced by `sunspot prove`.
    pub fn proof_path(&self) -> String {
        format!("{}/target/withdrawal.proof", self.circuit_dir)
    }

    /// Path to the public witness file produced by `sunspot prove`.
    pub fn pw_path(&self) -> String {
        format!("{}/target/withdrawal.pw", self.circuit_dir)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        // Note: env vars may be set in the process; this test only checks
        // that from_env() succeeds with defaults available.
        let cfg = Config::from_env().expect("config should load");
        assert_eq!(cfg.port, 4002);
        assert_eq!(cfg.circuit_dir, "/home/ubuntu/circuits/withdrawal");
        assert_eq!(cfg.nargo_bin, "nargo");
        assert_eq!(cfg.sunspot_bin, "sunspot");
    }

    #[test]
    fn test_artifact_paths() {
        let cfg = Config {
            port: 4002,
            circuit_dir: "/tmp/test".to_string(),
            nargo_bin: "nargo".to_string(),
            sunspot_bin: "sunspot".to_string(),
            nargo_timeout_secs: 30,
            sunspot_timeout_secs: 30,
        };
        assert_eq!(cfg.acir_path(), "/tmp/test/target/withdrawal.json");
        assert_eq!(cfg.ccs_path(), "/tmp/test/target/withdrawal.ccs");
        assert_eq!(cfg.pk_path(), "/tmp/test/target/withdrawal.pk");
        assert_eq!(cfg.proof_path(), "/tmp/test/target/withdrawal.proof");
        assert_eq!(cfg.pw_path(), "/tmp/test/target/withdrawal.pw");
    }
}
