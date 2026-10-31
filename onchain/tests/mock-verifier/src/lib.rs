//! Mock verifier program for LiteSVM tests.
//!
//! ## Why this exists
//!
//! The real Sunspot Groth16 verifier requires nargo + sunspot + the
//! gnark-solana crate to build. That is a heavy toolchain which CI does
//! not carry. The LiteSVM integration tests (`tests/src/test_withdraw.rs`,
//! `tests/src/test_adversarial.rs`) never exercise real proof verification —
//! they check that:
//!
//!   - `withdraw` reaches the CPI to the verifier,
//!   - the on-chain validations before the CPI fire correctly,
//!   - the instruction data reaching the verifier has the expected length.
//!
//! For those purposes, a minimal program that always returns `Ok(())` is
//! sufficient. It is installed under the *real* verifier Program ID
//! (`5t51iu6apRxgLbt91eVZ6YYzHsnmBCVnLGqJtqdfMFWJ`) in the LiteSVM
//! environment, so the `#[account(address = VERIFIER_PROGRAM_ID)]`
//! constraint in `withdraw` is satisfied.
//!
//! ## Production path
//!
//! Real proof verification is exercised E2E on devnet — see Stage 15.9 and
//! `docs/notes/10-e2e.md`. CI does not (and should not) attempt it.

use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, pubkey::Pubkey,
};

entrypoint!(process_instruction);

/// Always returns `Ok(())`. No account or data inspection.
fn process_instruction(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> ProgramResult {
    Ok(())
}
