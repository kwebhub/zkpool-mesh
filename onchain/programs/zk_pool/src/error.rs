//! Program errors.

use anchor_lang::prelude::*;

#[error_code]
pub enum ZkPoolError {
    // ============================================================
    // Pool
    // ============================================================
    #[msg("Pool is already initialized")]
    PoolAlreadyInitialized,

    #[msg("Pool is not initialized")]
    PoolNotInitialized,

    // ============================================================
    // Deposit
    // ============================================================
    #[msg("Deposit amount is below the minimum")]
    DepositBelowMinimum,

    #[msg("Merkle tree is full")]
    TreeFull,

    #[msg("New root must differ from the current root")]
    RootUnchanged,

    // ============================================================
    // DepositSplit (Stage 15.5)
    // ============================================================
    #[msg("Split amount sum does not equal total_amount")]
    SplitSumMismatch,

    #[msg("Not enough room in the Merkle tree for all splits")]
    NotEnoughRoom,

    // ============================================================
    // Withdraw
    // ============================================================
    #[msg("Unknown Merkle root: must be in the recent roots history")]
    UnknownRoot,

    #[msg("Nullifier has already been used")]
    NullifierAlreadyUsed,

    #[msg("Recipient in instruction does not match recipient in proof")]
    RecipientMismatch,

    #[msg("Amount in instruction does not match amount in proof")]
    AmountMismatch,

    #[msg("Vault balance is insufficient")]
    InsufficientVaultBalance,

    // ============================================================
    // ZK proof / verifier
    // ============================================================
    #[msg("Proof verification failed")]
    ProofVerificationFailed,

    #[msg("Invalid proof length")]
    InvalidProofLength,

    #[msg("Invalid public inputs length")]
    InvalidPublicInputsLength,
}
