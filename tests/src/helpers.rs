//! Helpers for LiteSVM-based tests.
//!
//! This module provides:
//!   - `setup_svm()` — creates a LiteSVM instance with both programs loaded.
//!   - Loading of `zk_pool.so` and `withdrawal.so` from the build artifacts.
//!
//! ## Where the `.so` files come from
//!
//! Local (dev container): pre-built at
//!   - `/home/ubuntu/onchain/target/deploy/zk_pool.so`
//!   - `/home/ubuntu/circuits/withdrawal/target/withdrawal.so`
//!
//! CI (GitHub Actions): the LiteSVM job builds both programs before running
//! the tests, and the artifacts land in `target/` under the repo checkout.
//! Paths are resolved relative to `CARGO_MANIFEST_DIR`, which for this crate
//! is `tests/`.
//!
//! Environment overrides (optional):
//!   - `ZK_POOL_SO` — absolute path to the `zk_pool` BPF binary.
//!   - `VERIFIER_SO` — absolute path to the Sunspot verifier BPF binary.

use std::path::PathBuf;

use litesvm::LiteSVM;
use solana_address::Address;

/// Program IDs.
pub const ZK_POOL_ID: &str = "8cGzkFK9H15mcpndAaY7ApCJhkHcujttR4E2D8rS6LCm";
pub const VERIFIER_ID: &str = "5t51iu6apRxgLbt91eVZ6YYzHsnmBCVnLGqJtqdfMFWJ";

/// Absolute path to `tests/` — the directory containing this crate's
/// `Cargo.toml`.
fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Resolve the path to `zk_pool.so`.
///
/// Priority:
///   1. `ZK_POOL_SO` env var (absolute path).
///   2. `<repo>/onchain/target/deploy/zk_pool.so` (relative to `tests/`).
fn zk_pool_so_path() -> PathBuf {
    if let Ok(p) = std::env::var("ZK_POOL_SO") {
        return PathBuf::from(p);
    }
    manifest_dir()
        .join("..")
        .join("onchain/target/deploy/zk_pool.so")
}

/// Resolve the path to `withdrawal.so` (the Sunspot verifier).
///
/// Priority:
///   1. `VERIFIER_SO` env var (absolute path).
///   2. `<repo>/circuits/withdrawal/target/withdrawal.so` (relative to `tests/`).
fn verifier_so_path() -> PathBuf {
    if let Ok(p) = std::env::var("VERIFIER_SO") {
        return PathBuf::from(p);
    }
    manifest_dir()
        .join("..")
        .join("circuits/withdrawal/target/withdrawal.so")
}

/// Creates a LiteSVM instance with both programs loaded:
///   - `zk_pool`
///   - the Sunspot verifier (needed for `withdraw` CPI).
///
/// Panics with a descriptive message if either `.so` file is missing.
/// CI must build them before running this crate's tests.
pub fn setup_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();

    let zk_pool_id: Address = ZK_POOL_ID.parse().expect("valid zk_pool ID");
    let verifier_id: Address = VERIFIER_ID.parse().expect("valid verifier ID");

    let zk_pool_so = zk_pool_so_path();
    let verifier_so = verifier_so_path();

    if !zk_pool_so.exists() {
        panic!(
            "zk_pool.so not found at {}. \
             Build it with `anchor build` (local) or ensure CI builds it before running tests.",
            zk_pool_so.display()
        );
    }
    if !verifier_so.exists() {
        panic!(
            "withdrawal.so (verifier) not found at {}. \
             Build it with `sunspot deploy` (local) or ensure CI builds it before running tests.",
            verifier_so.display()
        );
    }

    svm.add_program_from_file(zk_pool_id, &zk_pool_so)
        .unwrap_or_else(|e| panic!("failed to load {}: {:?}", zk_pool_so.display(), e));

    svm.add_program_from_file(verifier_id, &verifier_so)
        .unwrap_or_else(|e| panic!("failed to load {}: {:?}", verifier_so.display(), e));

    svm
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_svm_loads_both_programs() {
        let svm = setup_svm();
        let zk_pool_id: Address = ZK_POOL_ID.parse().unwrap();
        let verifier_id: Address = VERIFIER_ID.parse().unwrap();

        assert!(svm.get_account(&zk_pool_id).is_some(), "zk_pool not loaded");
        assert!(
            svm.get_account(&verifier_id).is_some(),
            "verifier not loaded"
        );
    }
}
