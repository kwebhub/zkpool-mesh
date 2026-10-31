//! Shared Poseidon2 library for zkpool-solana.
//!
//! Provides `hash_1`, `hash_2`, `hash_3` for BN254 field elements.
//! Uses Noir's builtin `poseidon2_permutation` with state size t = 4.
//!
//! ## Zero-padding
//!
//! Poseidon2 with t = 4 has rate r = 3 and capacity c = 1.
//! We must NOT fill all four state slots — that sets capacity to zero
//! and destroys cryptographic security. Instead, we zero-pad:
//!
//!   hash_1(in)        = poseidon2_permutation([in, 0, 0, 0])[0]
//!   hash_2(in1, in2)  = poseidon2_permutation([in1, in2, 0, 0])[0]
//!   hash_3(in1, in2, in3) = poseidon2_permutation([in1, in2, in3, 0])[0]
//!
//! `hash_4` is intentionally absent — it would require filling all four slots.
//!
//! ## Known property: no domain separation
//!
//! Because zero-padding is used, the following identities hold:
//!
//!   hash_1(x)       == hash_2(x, 0)
//!   hash_2(x, y)    == hash_3(x, y, 0)
//!
//! This is **not** a bug — it is a direct consequence of zero-padding.
//! The zkpool-solana protocol never hashes the same value with two
//! different arities in the same context, so no collision is possible
//! in practice:
//!
//!   - `hash_1` is used only for nullifier_hash.
//!   - `hash_2` is used only for recipient_binding and Merkle nodes.
//!   - `hash_3` is used only for commitment.
//!
//! If domain separation is ever needed, it must be added explicitly
//! (e.g. via distinct tags) and the spec must be updated accordingly.

use std::hash::poseidon2_permutation;

/// Poseidon2 hash of a single field element.
///
/// State: [input, 0, 0, 0]. Returns state[0] after permutation.
pub fn hash_1(input: Field) -> Field {
    let state = [input, 0, 0, 0];
    poseidon2_permutation(state)[0]
}

/// Poseidon2 hash of two field elements.
///
/// State: [input1, input2, 0, 0]. Returns state[0] after permutation.
///
/// NOTE: Poseidon2 is NOT commutative. `hash_2(a, b) != hash_2(b, a)`.
pub fn hash_2(input1: Field, input2: Field) -> Field {
    let state = [input1, input2, 0, 0];
    poseidon2_permutation(state)[0]
}

/// Poseidon2 hash of three field elements.
///
/// State: [input1, input2, input3, 0]. Returns state[0] after permutation.
pub fn hash_3(input1: Field, input2: Field, input3: Field) -> Field {
    let state = [input1, input2, input3, 0];
    poseidon2_permutation(state)[0]
}

// ============================================================
// Tests
// ============================================================

#[test]
fn test_hash_1_deterministic() {
    let a = hash_1(42);
    let b = hash_1(42);
    assert(a == b);
}

#[test]
fn test_hash_2_deterministic() {
    let a = hash_2(1, 2);
    let b = hash_2(1, 2);
    assert(a == b);
}

#[test]
fn test_hash_3_deterministic() {
    let a = hash_3(1, 2, 3);
    let b = hash_3(1, 2, 3);
    assert(a == b);
}

#[test]
fn test_hash_1_differs_for_different_inputs() {
    let a = hash_1(1);
    let b = hash_1(2);
    assert(a != b);
}

#[test]
fn test_hash_2_differs_for_different_inputs() {
    let a = hash_2(1, 2);
    let b = hash_2(2, 1);
    assert(a != b);
}

#[test]
fn test_hash_2_not_commutative() {
    // Poseidon2 is not commutative — this is critical for Merkle tree correctness.
    let a = hash_2(1, 2);
    let b = hash_2(2, 1);
    assert(a != b);
}

#[test]
fn test_hash_3_not_commutative_with_first_two() {
    let a = hash_3(1, 2, 3);
    let b = hash_3(2, 1, 3);
    assert(a != b);
}

#[test]
fn test_hash_3_not_commutative_with_last_two() {
    let a = hash_3(1, 2, 3);
    let b = hash_3(1, 3, 2);
    assert(a != b);
}

#[test]
fn test_zero_padding_collision_hash_1_and_hash_2() {
    // Documented property: hash_1(x) == hash_2(x, 0) because of zero-padding.
    // This test asserts the property so that any change (e.g. adding domain
    // separation) will be caught and reviewed deliberately.
    let a = hash_1(9);
    let b = hash_2(9, 0);
    assert(a == b);
}

#[test]
fn test_zero_padding_collision_hash_2_and_hash_3() {
    // Documented property: hash_2(x, y) == hash_3(x, y, 0).
    let a = hash_2(5, 7);
    let b = hash_3(5, 7, 0);
    assert(a == b);
}

#[test]
fn test_hash_1_nonzero_differs_from_hash_2_nonzero() {
    // When the extra inputs are NOT zero, the hashes differ.
    // This is the property we actually rely on.
    let a = hash_1(9);
    let b = hash_2(9, 1);
    assert(a != b);
}
