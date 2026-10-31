//! `hashes` circuit — commitment and nullifier_hash.
//!
//! Purpose:
//!   This circuit is compiled to ACIR (`hashes.json`) and loaded by
//!   the frontend via `@noir-lang/noir_js`. It lets the browser compute:
//!
//!     commitment      = hash_3(nullifier, secret, amount)
//!     nullifier_hash  = hash_1(nullifier)
//!
//!   Using the SAME ACIR as the withdrawal circuit guarantees that the
//!   values computed in the browser match byte-for-byte what the withdrawal
//!   circuit would compute — which is what makes the eventual proof valid.
//!
//! Interface:
//!   Inputs:  nullifier: Field, secret: Field, amount: Field
//!   Outputs: pub (commitment: Field, nullifier_hash: Field)
//!
//! No public inputs. The output tuple is marked `pub` because Noir requires
//! `pub` on the entry-point return type — verifiers cannot retrieve private
//! witness, so a non-pub return would be unreachable. This does not affect
//! the withdrawal circuit: there, commitment is private, and only
//! `nullifier_hash` is exposed as a public input.

use poseidon::{hash_1, hash_3};

fn main(nullifier: Field, secret: Field, amount: Field) -> pub (Field, Field) {
    let commitment = hash_3(nullifier, secret, amount);
    let nullifier_hash = hash_1(nullifier);
    (commitment, nullifier_hash)
}

// ============================================================
// Tests
// ============================================================

#[test]
fn test_hashes_deterministic() {
    let (c1, n1) = main(1, 2, 3);
    let (c2, n2) = main(1, 2, 3);
    assert(c1 == c2);
    assert(n1 == n2);
}

#[test]
fn test_commitment_differs_for_different_amount() {
    let (c1, _) = main(1, 2, 3);
    let (c2, _) = main(1, 2, 4);
    assert(c1 != c2);
}

#[test]
fn test_commitment_differs_for_different_secret() {
    let (c1, _) = main(1, 2, 3);
    let (c2, _) = main(1, 5, 3);
    assert(c1 != c2);
}

#[test]
fn test_commitment_differs_for_different_nullifier() {
    let (c1, _) = main(1, 2, 3);
    let (c2, _) = main(9, 2, 3);
    assert(c1 != c2);
}

#[test]
fn test_nullifier_hash_depends_only_on_nullifier() {
    let (_, n1) = main(1, 2, 3);
    let (_, n2) = main(1, 5, 7);
    // Same nullifier → same nullifier_hash, regardless of secret and amount.
    assert(n1 == n2);
}

#[test]
fn test_nullifier_hash_differs_for_different_nullifier() {
    let (_, n1) = main(1, 2, 3);
    let (_, n2) = main(2, 2, 3);
    assert(n1 != n2);
}

#[test]
fn test_commitment_and_nullifier_hash_are_different() {
    let (c, n) = main(1, 2, 3);
    assert(c != n);
}

#[test]
fn test_matches_poseidon_library() {
    let nullifier: Field = 42;
    let secret: Field = 1000;
    // amount in lamports: 1_000_000 = 0.001 SOL
    let amount: Field = 1_000_000;
    let (c, n) = main(nullifier, secret, amount);
    let expected_c = poseidon::hash_3(nullifier, secret, amount);
    let expected_n = poseidon::hash_1(nullifier);
    assert(c == expected_c);
    assert(n == expected_n);
}

#[test]
fn test_typical_amount_lamports() {
    // 0.001 SOL = 1_000_000 lamports
    let (c1, _) = main(1, 2, 1_000_000);
    let (c2, _) = main(1, 2, 2_000_000);
    assert(c1 != c2);
}
