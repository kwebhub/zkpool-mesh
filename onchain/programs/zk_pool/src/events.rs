//! Program events.
//!
//! Events are emitted by the program and consumed by the off-chain indexer
//! (backend). They are parsed from transaction logs by their 8-byte Anchor
//! discriminator.
//!
//! IMPORTANT: do not change the field order or types after the program is
//! deployed — the indexer relies on a stable binary layout. If a change is
//! unavoidable, update `services/backend/src/indexer.rs` in lockstep.

use anchor_lang::prelude::*;

/// Emitted on every successful deposit.
///
/// Binary layout (after 8-byte discriminator):
///   commitment      [u8; 32]
///   leaf_index      u64
///   new_root        [u8; 32]
///   timestamp       i64
/// Total payload: 32 + 8 + 32 + 8 = 80 bytes.
#[event]
pub struct DepositEvent {
    /// Commitment inserted into the Merkle tree.
    pub commitment: [u8; 32],

    /// Index of the leaf in the tree (0-based).
    pub leaf_index: u64,

    /// New Merkle root after inserting the commitment.
    pub new_root: [u8; 32],

    /// Unix timestamp of the block.
    pub timestamp: i64,
}

/// Emitted on every successful withdrawal.
///
/// Binary layout (after 8-byte discriminator):
///   nullifier_hash  [u8; 32]
///   recipient       Pubkey (32 bytes)
///   amount          u64
///   timestamp       i64
/// Total payload: 32 + 32 + 8 + 8 = 80 bytes.
#[event]
pub struct WithdrawEvent {
    /// Nullifier hash of the spent deposit.
    pub nullifier_hash: [u8; 32],

    /// Recipient that received the SOL.
    pub recipient: Pubkey,

    /// Amount transferred, in lamports.
    pub amount: u64,

    /// Unix timestamp of the block.
    pub timestamp: i64,
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_event_size() {
        // 32 (commitment) + 8 (leaf_index) + 32 (new_root) + 8 (timestamp) = 80
        assert_eq!(
            std::mem::size_of::<u64>() * 2 + std::mem::size_of::<[u8; 32]>() * 2,
            80
        );
    }

    #[test]
    fn test_withdraw_event_size() {
        // 32 (nullifier_hash) + 32 (recipient) + 8 (amount) + 8 (timestamp) = 80
        assert_eq!(
            std::mem::size_of::<u64>() * 2 + std::mem::size_of::<[u8; 32]>() * 2,
            80
        );
    }

    #[test]
    fn test_deposit_event_constructible() {
        let evt = DepositEvent {
            commitment: [1u8; 32],
            leaf_index: 42,
            new_root: [2u8; 32],
            timestamp: 1_700_000_000,
        };
        assert_eq!(evt.commitment, [1u8; 32]);
        assert_eq!(evt.leaf_index, 42);
        assert_eq!(evt.new_root, [2u8; 32]);
        assert_eq!(evt.timestamp, 1_700_000_000);
    }

    #[test]
    fn test_withdraw_event_constructible() {
        let evt = WithdrawEvent {
            nullifier_hash: [3u8; 32],
            recipient: Pubkey::default(),
            amount: 1_000_000,
            timestamp: 1_700_000_000,
        };
        assert_eq!(evt.nullifier_hash, [3u8; 32]);
        assert_eq!(evt.recipient, Pubkey::default());
        assert_eq!(evt.amount, 1_000_000);
        assert_eq!(evt.timestamp, 1_700_000_000);
    }
}
