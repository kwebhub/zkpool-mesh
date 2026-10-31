//! LiteSVM integration tests for zk_pool.
//!
//! This is a standalone crate (not in the `onchain/` workspace). It uses
//! a newer Rust toolchain (1.98.1) because `litesvm` requires Agave 4.2+.
//!
//! Tests run entirely in-process against a local Solana VM. They do not
//! require a validator, devnet, or SOL. Two programs are loaded:
//!   - `zk_pool` (the pool itself),
//!   - `sunspot_verifier` (the Groth16 verifier, compiled in stage 3.3).

#[cfg(test)]
mod helpers;

#[cfg(test)]
mod test_pool;

#[cfg(test)]
mod test_deposit;

#[cfg(test)]
mod test_deposit_split;

#[cfg(test)]
mod test_withdraw;

#[cfg(test)]
mod test_double_spend;

#[cfg(test)]
pub mod test_adversarial;
