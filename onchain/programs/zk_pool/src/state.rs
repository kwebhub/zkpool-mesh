//! State accounts.
//!
//! Two account types:
//!   - `PoolState` — the pool metadata: authority, Merkle tree state, deposit counter.
//!   - `NullifierRecord` — a per-nullifier marker that prevents double-spend.

use anchor_lang::prelude::*;

use crate::constants::{MAX_LEAVES, ROOT_HISTORY_SIZE};

/// Pool state account.
///
/// PDA seeds: `[POOL_SEED]`.
///
/// Contains:
///   - authority:           who created the pool (for future admin actions).
///   - next_leaf_index:     the next free index in the Merkle tree.
///   - total_deposits:      monotonic counter of successful deposits.
///   - current_root_index:  index into `roots` of the most recent root.
///   - roots:               ring buffer of the last ROOT_HISTORY_SIZE roots.
///
/// Size: 8 + 32 + 8 + 8 + 8 + (10 × 32) = 384 bytes.
#[account]
#[derive(InitSpace)]
pub struct PoolState {
    /// Pool creator. Will be used for future admin actions (pause, upgrade).
    pub authority: Pubkey,

    /// Next free leaf index in the Merkle tree.
    /// Invariant: `next_leaf_index < MAX_LEAVES`.
    pub next_leaf_index: u64,

    /// Total number of successful deposits.
    pub total_deposits: u64,

    /// Index in `roots` of the most recent root.
    pub current_root_index: u64,

    /// Ring buffer of the last `ROOT_HISTORY_SIZE` roots.
    /// Empty slots are all-zero and are treated as "not present".
    pub roots: [[u8; 32]; ROOT_HISTORY_SIZE],
}

impl PoolState {
    /// Returns `true` if `root` is in the recent roots history.
    ///
    /// Iterates over all `ROOT_HISTORY_SIZE` slots. All-zero roots are
    /// skipped (they represent empty slots before any deposit).
    pub fn is_known_root(&self, root: &[u8; 32]) -> bool {
        if *root == [0u8; 32] {
            return false;
        }
        self.roots.iter().any(|r| r == root)
    }

    /// Adds `new_root` to the ring buffer, moving the cursor forward.
    pub fn add_root(&mut self, new_root: [u8; 32]) {
        let next_index = ((self.current_root_index + 1) as usize) % ROOT_HISTORY_SIZE;
        self.roots[next_index] = new_root;
        self.current_root_index = next_index as u64;
    }

    /// Returns the current (most recent) root.
    pub fn current_root(&self) -> [u8; 32] {
        self.roots[self.current_root_index as usize]
    }

    /// Returns `true` if the tree has room for another leaf.
    pub fn has_room(&self) -> bool {
        self.next_leaf_index < MAX_LEAVES
    }
}

/// Per-nullifier marker that prevents double-spend.
///
/// PDA seeds: `[NULLIFIER_RECORD_SEED, pool, nullifier_hash]`.
///
/// If this account exists, the nullifier has been used. The `init`
/// constraint in the `withdraw` instruction will fail if the account
/// already exists — that is how double-spend is prevented.
///
/// Size: 8 + 32 + 32 + 8 + 8 = 88 bytes.
#[account]
#[derive(InitSpace)]
pub struct NullifierRecord {
    /// Pool this nullifier belongs to.
    pub pool: Pubkey,

    /// The nullifier hash that was spent.
    pub nullifier_hash: [u8; 32],

    /// Recipient that received the SOL.
    pub recipient: Pubkey,

    /// Amount transferred, in lamports.
    pub amount: u64,

    /// Unix timestamp of the withdrawal.
    pub timestamp: i64,
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::prelude::Pubkey;

    fn empty_pool() -> PoolState {
        PoolState {
            authority: Pubkey::default(),
            next_leaf_index: 0,
            total_deposits: 0,
            current_root_index: 0,
            roots: [[0u8; 32]; ROOT_HISTORY_SIZE],
        }
    }

    #[test]
    fn test_pool_state_init_space() {
        // 32 (authority) + 8 (next_leaf_index) + 8 (total_deposits)
        // + 8 (current_root_index) + 10 * 32 (roots) = 376
        // Anchor adds 8-byte discriminator on top; INIT_SPACE = 376.
        assert_eq!(PoolState::INIT_SPACE, 32 + 8 + 8 + 8 + (10 * 32));
        assert_eq!(PoolState::INIT_SPACE, 376);
    }

    #[test]
    fn test_nullifier_record_init_space() {
        // 32 (pool) + 32 (nullifier_hash) + 32 (recipient) + 8 (amount) + 8 (timestamp) = 112
        assert_eq!(NullifierRecord::INIT_SPACE, 32 + 32 + 32 + 8 + 8);
        assert_eq!(NullifierRecord::INIT_SPACE, 112);
    }

    #[test]
    fn test_is_known_root_empty() {
        let pool = empty_pool();
        assert!(!pool.is_known_root(&[0u8; 32]));
        assert!(!pool.is_known_root(&[1u8; 32]));
    }

    #[test]
    fn test_add_root_and_is_known() {
        let mut pool = empty_pool();
        let root_a = [0xaau8; 32];
        pool.add_root(root_a);
        assert!(pool.is_known_root(&root_a));
        assert_eq!(pool.current_root(), root_a);
    }

    #[test]
    fn test_add_multiple_roots_ring_buffer() {
        let mut pool = empty_pool();
        for i in 0..15u8 {
            let mut root = [0u8; 32];
            root[0] = i + 1; // avoid all-zero
            pool.add_root(root);
        }
        // Only the last ROOT_HISTORY_SIZE (10) roots should be present.
        for i in 5..15u8 {
            let mut root = [0u8; 32];
            root[0] = i + 1;
            assert!(pool.is_known_root(&root), "root #{} should be known", i + 1);
        }
        // The first few should have been overwritten.
        for i in 0..5u8 {
            let mut root = [0u8; 32];
            root[0] = i + 1;
            assert!(
                !pool.is_known_root(&root),
                "root #{} should be forgotten",
                i + 1
            );
        }
    }

    #[test]
    fn test_current_root_after_add() {
        let mut pool = empty_pool();
        assert_eq!(pool.current_root(), [0u8; 32]);
        pool.add_root([1u8; 32]);
        assert_eq!(pool.current_root(), [1u8; 32]);
        pool.add_root([2u8; 32]);
        assert_eq!(pool.current_root(), [2u8; 32]);
    }

    #[test]
    fn test_has_room_empty() {
        let pool = empty_pool();
        assert!(pool.has_room());
    }

    #[test]
    fn test_has_room_at_limit() {
        let mut pool = empty_pool();
        pool.next_leaf_index = MAX_LEAVES;
        assert!(!pool.has_room());
    }

    #[test]
    fn test_has_room_just_below_limit() {
        let mut pool = empty_pool();
        pool.next_leaf_index = MAX_LEAVES - 1;
        assert!(pool.has_room());
    }

    #[test]
    fn test_is_known_root_rejects_zero() {
        let mut pool = empty_pool();
        // Even if we explicitly try to add [0; 32], is_known_root returns false
        // because zero roots are treated as empty slots.
        pool.add_root([0u8; 32]);
        assert!(!pool.is_known_root(&[0u8; 32]));
    }
}
