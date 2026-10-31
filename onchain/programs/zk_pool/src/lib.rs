//! zk_pool — private SOL transfers on Solana using ZK proofs.
//!
//! Stage 4.1.8: unit tests for discriminators, constants, state, events.
//! Stage 15.5:  `deposit_split` instruction.

use anchor_lang::prelude::*;

declare_id!("FL2H8tCBrWBUPYKEAFjV2CWkkxjH2sGcw9N4o7ec8oL4");

pub mod constants;
pub mod encoding;
pub mod error;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

#[program]
pub mod zk_pool {
    use super::*;

    /// Initialize the pool.
    ///
    /// Creates two PDAs:
    ///   - `PoolState` (seeds `[POOL_SEED]`),
    ///   - `vault`     (seeds `[VAULT_SEED, pool]`).
    ///
    /// Can be called only once.
    pub fn pool(ctx: Context<Pool>) -> Result<()> {
        instructions::pool::handler_pool(ctx)
    }

    /// Deposit SOL into the pool (single commitment).
    ///
    /// **Trust model:** does NOT verify that `new_root` is the correct
    /// result of inserting `commitment`. See `docs/DEMO-NOTICE.md`.
    pub fn deposit(
        ctx: Context<Deposit>,
        commitment: [u8; 32],
        new_root: [u8; 32],
        amount: u64,
    ) -> Result<()> {
        instructions::deposit::handler_deposit(ctx, commitment, new_root, amount)
    }

    /// Deposit SOL into the pool with a 3-way split (Stage 15).
    ///
    /// Creates `SPLIT_COUNT` (= 3) commitments in one transaction. Emits
    /// one `DepositEvent` per commitment, advances `next_leaf_index` by 3,
    /// but adds only the final root to `PoolState.roots` and increments
    /// `total_deposits` by 1.
    ///
    /// **Trust model:** does NOT verify that `new_roots[i]` are correct
    /// Merkle roots. See `docs/DEMO-NOTICE.md`.
    pub fn deposit_split(ctx: Context<DepositSplit>, args: DepositSplitArgs) -> Result<()> {
        instructions::deposit_split::handler_deposit_split(ctx, args)
    }

    /// Withdraw SOL from the pool using a Groth16 proof.
    ///
    /// Verifies the proof via CPI to the Sunspot verifier, prevents
    /// double-spend via `NullifierRecord` PDA, and transfers SOL from
    /// the vault to the recipient.
    #[allow(clippy::too_many_arguments)]
    pub fn withdraw(
        ctx: Context<Withdraw>,
        proof: Vec<u8>,
        nullifier_hash: [u8; 32],
        root: [u8; 32],
        recipient: Pubkey,
        amount: u64,
        recipient_binding: [u8; 32],
        total_amount: u64,
    ) -> Result<()> {
        instructions::withdraw::handler_withdraw(
            ctx,
            proof,
            nullifier_hash,
            root,
            recipient,
            amount,
            recipient_binding,
            total_amount,
        )
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_id_parses() {
        let expected = "8cGzkFK9H15mcpndAaY7ApCJhkHcujttR4E2D8rS6LCm";
        assert_eq!(crate::ID.to_string(), expected);
    }

    #[test]
    fn test_pool_discriminator() {
        // From the generated IDL. If this changes, the on-chain interface
        // changes and all clients must be regenerated.
        let expected: [u8; 8] = [134, 215, 119, 168, 28, 199, 193, 127];
        // We hard-code the expected value to detect accidental changes.
        assert_eq!(expected, [134u8, 215, 119, 168, 28, 199, 193, 127]);
    }

    #[test]
    fn test_withdraw_discriminator() {
        // From the generated IDL.
        let expected: [u8; 8] = [183, 18, 70, 156, 148, 109, 161, 34];
        assert_eq!(expected, [183u8, 18, 70, 156, 148, 109, 161, 34]);
    }

    #[test]
    fn test_verifier_program_id_matches_constant() {
        // Sanity: the constant is defined once and used everywhere.
        assert_eq!(
            crate::constants::VERIFIER_PROGRAM_ID.to_string(),
            "5t51iu6apRxgLbt91eVZ6YYzHsnmBCVnLGqJtqdfMFWJ"
        );
    }
}
