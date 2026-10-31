//! Merkle tree root computation for the withdrawal circuit.
//!
//! Given a leaf, a Merkle proof (sibling hashes), and side flags, recompute
//! the root and return it. The caller compares it to the public `root` input.
//!
//! ## Why this module is separate from main.nr
//!
//! - It is a **library** function, not a circuit entry point.
//! - It is tested independently.
//! - `main.nr` uses it via `mod merkle_tree; use merkle_tree::compute_merkle_root;`.
//!
//! ## Ordering matters
//!
//! Poseidon2 is NOT commutative: `hash_2(a, b) != hash_2(b, a)`. If we swap
//! `left` and `right`, the computed root will not match the real root, and
//! the constraint in `main.nr` will fail. The `is_even[i]` flag encodes
//! whether our node is the left child (`true`) or the right child (`false`)
//! at level `i`.

use poseidon::hash_2;

/// Computes the Merkle root from a leaf and its proof.
///
/// # Arguments
/// - `leaf`: the starting leaf value (typically a commitment).
/// - `path`: sibling hashes at each level, from leaf to root.
///          `path[0]` is the sibling of `leaf`.
///          `path[DEPTH-1]` is the sibling at the top level.
/// - `is_even`: flags encoding the side of our node at each level.
///              `is_even[i] == true`  → our node is the LEFT child, sibling is RIGHT.
///              `is_even[i] == false` → our node is the RIGHT child, sibling is LEFT.
///
/// # Returns
/// The Merkle root.
///
/// # Const generic
/// `DEPTH` is a compile-time parameter (20 for zkpool-solana).
pub fn compute_merkle_root<let DEPTH: u32>(
    leaf: Field,
    path: [Field; DEPTH],
    is_even: [bool; DEPTH],
) -> Field {
    let mut current = leaf;
    for i in 0..DEPTH {
        let sibling = path[i];
        let (left, right) = if is_even[i] {
            (current, sibling)
        } else {
            (sibling, current)
        };
        current = hash_2(left, right);
    }
    current
}

// ============================================================
// Tests
// ============================================================

#[test]
fn test_single_level_root_left_child() {
    // Leaf is left child: hash_2(leaf, sibling).
    let leaf: Field = 100;
    let sibling: Field = 200;
    let path = [sibling];
    let is_even = [true];
    let root = compute_merkle_root::<1>(leaf, path, is_even);
    assert(root == hash_2(leaf, sibling));
}

#[test]
fn test_single_level_root_right_child() {
    // Leaf is right child: hash_2(sibling, leaf).
    let leaf: Field = 100;
    let sibling: Field = 200;
    let path = [sibling];
    let is_even = [false];
    let root = compute_merkle_root::<1>(leaf, path, is_even);
    assert(root == hash_2(sibling, leaf));
}

#[test]
fn test_single_level_left_and_right_differ() {
    let leaf: Field = 100;
    let sibling: Field = 200;
    let left_root = compute_merkle_root::<1>(leaf, [sibling], [true]);
    let right_root = compute_merkle_root::<1>(leaf, [sibling], [false]);
    assert(left_root != right_root);
}

#[test]
fn test_two_levels() {
    // Level 0: leaf is LEFT, sibling = s0 → P0 = hash_2(leaf, s0)
    // Level 1: P0 is RIGHT, sibling = s1 → root = hash_2(s1, P0)
    let leaf: Field = 1;
    let s0: Field = 2;
    let s1: Field = 3;
    let path = [s0, s1];
    let is_even = [true, false];
    let expected_p0 = hash_2(leaf, s0);
    let expected_root = hash_2(s1, expected_p0);
    let root = compute_merkle_root::<2>(leaf, path, is_even);
    assert(root == expected_root);
}

#[test]
fn test_deterministic() {
    let leaf: Field = 42;
    let path = [1, 2, 3];
    let is_even = [true, false, true];
    let r1 = compute_merkle_root::<3>(leaf, path, is_even);
    let r2 = compute_merkle_root::<3>(leaf, path, is_even);
    assert(r1 == r2);
}

#[test]
fn test_different_leaf_different_root() {
    let path = [1, 2, 3];
    let is_even = [true, false, true];
    let r1 = compute_merkle_root::<3>(10, path, is_even);
    let r2 = compute_merkle_root::<3>(11, path, is_even);
    assert(r1 != r2);
}

#[test]
fn test_flipping_one_flag_changes_root() {
    let leaf: Field = 42;
    let path = [1, 2, 3];
    let is_even_a = [true, false, true];
    let is_even_b = [true, false, false];
    let r1 = compute_merkle_root::<3>(leaf, path, is_even_a);
    let r2 = compute_merkle_root::<3>(leaf, path, is_even_b);
    assert(r1 != r2);
}

#[test]
fn test_flipping_one_sibling_changes_root() {
    let leaf: Field = 42;
    let is_even = [true, false, true];
    let r1 = compute_merkle_root::<3>(leaf, [1, 2, 3], is_even);
    let r2 = compute_merkle_root::<3>(leaf, [1, 2, 4], is_even);
    assert(r1 != r2);
}
