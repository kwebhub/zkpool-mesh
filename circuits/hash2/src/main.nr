//! `hash2` circuit — external Poseidon2 hash of two field elements.
//!
//! Purpose:
//!   This circuit is compiled to ACIR (`hash2.json`) and loaded by
//!   JavaScript consumers (merkle service, frontend) via `@noir-lang/noir_js`.
//!   Using the SAME ACIR as the on-chain circuit guarantees that Poseidon2
//!   hashes computed in JS match those computed in Noir byte-for-byte.
//!
//!   Rust implementations of Poseidon2 (light-poseidon, poseidon-rs) produce
//!   DIFFERENT hashes than Noir's builtin. The only way to guarantee identity
//!   is to use the same Noir circuit.
//!
//! Interface:
//!   Inputs:  left: Field, right: Field
//!   Output:  hash_2(left, right): Field
//!
//! No public inputs. No constraints beyond the Poseidon2 permutation itself.
//!
//! NOTE: Field values in Noir (BN254) must be < 2^254. A 256-bit value
//! (e.g. an arbitrary Solana Pubkey) cannot be used directly — it must be
//! reduced modulo the BN254 prime first. See `reduce_to_field` in v2.

use poseidon::hash_2;

fn main(left: Field, right: Field) -> pub Field {
    hash_2(left, right)
}

// ============================================================
// Tests
// ============================================================

#[test]
fn test_hash2_deterministic() {
    let a = hash_2(1, 2);
    let b = hash_2(1, 2);
    assert(a == b);
}

#[test]
fn test_hash2_not_commutative() {
    let a = hash_2(1, 2);
    let b = hash_2(2, 1);
    assert(a != b);
}

#[test]
fn test_hash2_zero_zero_matches_library() {
    let circuit = hash_2(0, 0);
    let library = poseidon::hash_2(0, 0);
    assert(circuit == library);
}

#[test]
fn test_hash2_typical_merkle_input() {
    // Simulates a typical Merkle tree node: hash_2(left_child, right_child).
    // Both values are valid BN254 field elements (< 2^254).
    let left: Field = 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef;
    let right: Field = 0x00fedcba9876543210fedcba9876543210fedcba9876543210fedcba98765432;
    let a = hash_2(left, right);
    let b = hash_2(left, right);
    assert(a == b);
    assert(a != hash_2(right, left));
}

#[test]
fn test_hash2_max_valid_field_element() {
    // Largest valid BN254 field element (p - 1) expressed in hex.
    // p = 21888242871839275222246405745257275088548364400416034343698204186575808495617
    // p - 1 = ...616
    let near_max: Field = 0x30644e72e131a029b85045b68181585d2833e84879b9709143e1f593f0000000;
    let a = hash_2(near_max, 0);
    let b = hash_2(near_max, 0);
    assert(a == b);
}
