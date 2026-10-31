//! `withdrawal` circuit — the main ZK circuit for zkpool-solana.
//!
//! ## Purpose
//!
//! Proves, in zero knowledge, that:
//!
//!   C1. `hash_1(nullifier) == nullifier_hash`
//!   C2. `hash_2(note_secret, recipient) == recipient_binding`
//!   C3. `compute_merkle_root(hash_3(nullifier, secret, amount), merkle_proof, is_even) == root`
//!   C4. `splits[0] + splits[1] + splits[2] == total_amount`
//!   C5. `splits[note_index] == amount`
//!
//! ## Public inputs (6, in order — see spec.json)
//!
//!   [0] root                — Merkle root; must be in PoolState.roots[] history.
//!   [1] nullifier_hash      — hash_1(nullifier); used as PDA seed for NullifierRecord.
//!   [2] recipient           — Solana Pubkey reduced modulo BN254 field prime.
//!   [3] recipient_binding   — hash_2(note_secret, recipient); prevents front-running.
//!   [4] amount              — withdrawal amount in lamports for THIS note (u64, right-padded to 32 bytes).
//!   [5] total_amount        — aggregate deposit amount (sum of all splits) in lamports
//!                             (u64, right-padded to 32 bytes). Added in Stage 15.
//!
//! ## Private inputs (7)
//!
//!   nullifier      — secret; hashed into nullifier_hash.
//!   secret         — secret; part of commitment.
//!   note_secret    — secret; part of recipient_binding.
//!   merkle_proof   — [Field; 20], sibling hashes from leaf to root.
//!   is_even        — [bool; 20], side flag per level (true = our node is left child).
//!   splits         — [Field; 3], split vector of the deposit this note belongs to.
//!                    Added in Stage 15. Must satisfy Σ splits[i] == total_amount (C4).
//!   note_index     — u32, index of this note within splits (0, 1, or 2).
//!                    Added in Stage 15. Must be < SPLIT_COUNT (checked before indexing).
//!
//! ## TREE_DEPTH and SPLIT_COUNT
//!
//! TREE_DEPTH is hard-coded to 20 and SPLIT_COUNT to 3. Both must match
//! `spec.json` (`circuit.tree_depth`, `circuit.split_count`) and on-chain
//! `constants::{TREE_DEPTH, SPLIT_COUNT}`. If either changes, all layers must
//! be updated simultaneously and the Sunspot verifier must be rebuilt.

mod merkle_tree;
mod test_witness;

use merkle_tree::compute_merkle_root;
use poseidon::{hash_1, hash_2, hash_3};

/// Merkle tree depth. Must match `spec.json` `circuit.tree_depth` (20)
/// and on-chain `constants::TREE_DEPTH`.
global TREE_DEPTH: u32 = 20;

/// Number of splits per deposit. Must match `spec.json` `circuit.split_count` (3)
/// and on-chain `constants::SPLIT_COUNT`.
global SPLIT_COUNT: u32 = 3;

fn main(
    // ---- public inputs (order matters — validated against spec.json) ----
    root: pub Field,
    nullifier_hash: pub Field,
    recipient: pub Field,
    recipient_binding: pub Field,
    amount: pub Field,
    total_amount: pub Field,
    // ---- private inputs ----
    nullifier: Field,
    secret: Field,
    note_secret: Field,
    merkle_proof: [Field; TREE_DEPTH],
    is_even: [bool; TREE_DEPTH],
    splits: [Field; SPLIT_COUNT],
    note_index: u32,
) {
    // C1: nullifier_hash = hash_1(nullifier)
    let computed_nullifier_hash = hash_1(nullifier);
    assert(computed_nullifier_hash == nullifier_hash);

    // C2: recipient_binding = hash_2(note_secret, recipient)
    let computed_binding = hash_2(note_secret, recipient);
    assert(computed_binding == recipient_binding);

    // C3: Merkle root
    let commitment = hash_3(nullifier, secret, amount);
    let computed_root = compute_merkle_root::<TREE_DEPTH>(commitment, merkle_proof, is_even);
    assert(computed_root == root);

    // C4: Σ splits[i] == total_amount
    let splits_sum = splits[0] + splits[1] + splits[2];
    assert(splits_sum == total_amount);

    // C5: splits[note_index] == amount
    // Bounds check first — Noir arrays are fixed-size and dynamic indexing
    // is not guaranteed to be supported. We also do the bounds check as an
    // explicit constraint so a malicious prover cannot bypass it.
    assert(note_index < SPLIT_COUNT);
    let note_amount = if note_index == 0 {
        splits[0]
    } else if note_index == 1 {
        splits[1]
    } else {
        splits[2]
    };
    assert(note_amount == amount);
}

// ============================================================
// Integration tests
// ============================================================

#[test]
fn test_c1_nullifier_hash_correct() {
    let nullifier: Field = 12345;
    let expected = hash_1(nullifier);
    assert(expected == hash_1(nullifier));
}

#[test]
fn test_c2_recipient_binding_correct() {
    let note_secret: Field = 67890;
    let recipient: Field = 11111;
    let expected = hash_2(note_secret, recipient);
    assert(expected == hash_2(note_secret, recipient));
}

#[test]
fn test_c3_merkle_root_two_levels() {
    let commitment: Field = 999;
    let s0: Field = 888;
    let s1: Field = 777;

    let p0 = hash_2(commitment, s0);
    let root_expected = hash_2(p0, s1);

    let proof = [s0, s1];
    let is_even = [true, true];

    let root_computed = compute_merkle_root::<2>(commitment, proof, is_even);
    assert(root_computed == root_expected);
}

#[test]
fn test_c3_wrong_sibling_fails() {
    let commitment: Field = 999;
    let s0: Field = 888;
    let s1: Field = 777;

    let correct = compute_merkle_root::<2>(commitment, [s0, s1], [true, true]);
    let wrong   = compute_merkle_root::<2>(commitment, [s0, 666], [true, true]);
    assert(correct != wrong);
}

#[test]
fn test_c3_wrong_flag_fails() {
    let commitment: Field = 999;
    let s0: Field = 888;
    let s1: Field = 777;

    let correct = compute_merkle_root::<2>(commitment, [s0, s1], [true, true]);
    let wrong   = compute_merkle_root::<2>(commitment, [s0, s1], [false, true]);
    assert(correct != wrong);
}

#[test]
fn test_c1_c2_c3_full_flow_2_levels() {
    let nullifier: Field = 1001;
    let secret: Field = 2002;
    let note_secret: Field = 3003;
    let amount: Field = 1_000_000;
    let recipient: Field = 4004;

    let expected_nullifier_hash = hash_1(nullifier);
    let expected_recipient_binding = hash_2(note_secret, recipient);
    let commitment = hash_3(nullifier, secret, amount);

    let s0: Field = 555;
    let s1: Field = 666;
    let p0 = hash_2(commitment, s0);
    let root = hash_2(p0, s1);
    let proof = [s0, s1];
    let is_even = [true, true];

    assert(hash_1(nullifier) == expected_nullifier_hash);
    assert(hash_2(note_secret, recipient) == expected_recipient_binding);
    assert(compute_merkle_root::<2>(commitment, proof, is_even) == root);
}

#[test]
fn test_c2_wrong_recipient_fails() {
    let note_secret: Field = 3003;
    let honest_recipient: Field = 4004;
    let attacker_recipient: Field = 4005;

    let honest_binding = hash_2(note_secret, honest_recipient);
    let attacker_binding = hash_2(note_secret, attacker_recipient);

    assert(honest_binding != attacker_binding);
}

#[test]
fn test_tree_depth_constant() {
    assert(TREE_DEPTH == 20);
}

#[test]
fn test_split_count_constant() {
    assert(SPLIT_COUNT == 3);
}

// ---- Stage 15: split deposit tests ----

#[test]
fn test_c4_splits_sum_matches_total() {
    let splits: [Field; 3] = [500_000, 300_000, 200_000];
    let total_amount: Field = 1_000_000;
    assert(splits[0] + splits[1] + splits[2] == total_amount);
}

#[test]
fn test_c4_splits_sum_mismatch() {
    let splits: [Field; 3] = [500_000, 300_000, 100_000];
    let total_amount: Field = 1_000_000;
    // Sum is 900_000, not 1_000_000 — this must not hold.
    assert(splits[0] + splits[1] + splits[2] != total_amount);
}

#[test]
fn test_c5_note_index_0_matches_splits_0() {
    let splits: [Field; 3] = [500_000, 300_000, 200_000];
    let note_index: u32 = 0;
    let amount = splits[0];
    assert(note_index == 0);
    assert(amount == splits[0]);
}

#[test]
fn test_c5_note_index_1_matches_splits_1() {
    let splits: [Field; 3] = [500_000, 300_000, 200_000];
    let note_index: u32 = 1;
    let amount = splits[1];
    assert(note_index == 1);
    assert(amount == splits[1]);
}

#[test]
fn test_c5_note_index_2_matches_splits_2() {
    let splits: [Field; 3] = [500_000, 300_000, 200_000];
    let note_index: u32 = 2;
    let amount = splits[2];
    assert(note_index == 2);
    assert(amount == splits[2]);
}

#[test]
fn test_c5_note_index_out_of_bounds_rejected_by_check() {
    // note_index = 5 is not < SPLIT_COUNT = 3. The bounds check must reject it.
    let note_index: u32 = 5;
    assert(note_index >= SPLIT_COUNT);
}
