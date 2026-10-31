//! Public inputs encoding for the verifier program.
//!
//! The `withdraw` instruction calls the verifier via CPI. The verifier
//! expects a single blob of bytes: a 12-byte header followed by 6 × 32-byte
//! public inputs (204 bytes total).
//!
//! The layout is defined in `circuits/withdrawal/spec.json` (section
//! `witness_layout`) and must match:
//!   - `withdrawal.pw` produced by `sunspot prove` (stage 15.4),
//!   - the blob produced by the frontend (stage 15.8).
//!
//! If this layout ever changes, update `spec.json` and all three layers
//! simultaneously, then run `validate-spec`.
//!
//! ## Layout (Stage 15.4)
//!
//! ```text
//! [12-byte header]
//!   NR_PUBLIC_INPUTS (u32 BE) = 6
//!   0                 (u32 BE) = 0
//!   NR_PUBLIC_INPUTS (u32 BE) = 6
//! [6 × 32 bytes, in this order]
//!   root
//!   nullifier_hash
//!   recipient            (reduced to BN254)
//!   recipient_binding
//!   amount               (u64 BE, right-aligned in a 32-byte word)
//!   total_amount         (u64 BE, right-aligned in a 32-byte word)
//! ```
//!
//! Total: 12 + 6 × 32 = 204 bytes.

use anchor_lang::prelude::*;

use crate::constants::{NR_PUBLIC_INPUTS, PUBLIC_INPUTS_BYTES};

/// BN254 scalar field prime, big-endian bytes.
///
/// p = 21888242871839275222246405745257275088548364400416034343698204186575808495617
///
/// Hex (32 bytes):
///   0x30644e72e131a029b85045b68181585d2833e84879b9709143e1f593f0000001
pub const BN254_PRIME_BE: [u8; 32] = [
    0x30, 0x64, 0x4e, 0x72, 0xe1, 0x31, 0xa0, 0x29, 0xb8, 0x50, 0x45, 0xb6, 0x81, 0x81, 0x58, 0x5d,
    0x28, 0x33, 0xe8, 0x48, 0x79, 0xb9, 0x70, 0x91, 0x43, 0xe1, 0xf5, 0x93, 0xf0, 0x00, 0x00, 0x01,
];

/// Encodes the 6 public inputs into a 204-byte blob expected by the verifier.
///
/// Public inputs, in order:
///   [0] root
///   [1] nullifier_hash
///   [2] recipient             (Solana Pubkey, reduced to BN254)
///   [3] recipient_binding
///   [4] amount                (this note's amount, u64, right-aligned in 32 bytes)
///   [5] total_amount          (aggregate deposit amount, u64, right-aligned in 32 bytes)
#[allow(clippy::too_many_arguments)]
pub fn encode_public_inputs(
    root: &[u8; 32],
    nullifier_hash: &[u8; 32],
    recipient: &Pubkey,
    recipient_binding: &[u8; 32],
    amount: u64,
    total_amount: u64,
) -> [u8; PUBLIC_INPUTS_BYTES] {
    let mut out = [0u8; PUBLIC_INPUTS_BYTES];

    // ----- 12-byte header -----
    // [0..4]   NR_PUBLIC_INPUTS (u32 BE)
    // [4..8]   0                 (u32 BE)
    // [8..12]  NR_PUBLIC_INPUTS (u32 BE)
    out[0..4].copy_from_slice(&NR_PUBLIC_INPUTS.to_be_bytes());
    out[4..8].copy_from_slice(&0u32.to_be_bytes());
    out[8..12].copy_from_slice(&NR_PUBLIC_INPUTS.to_be_bytes());

    // ----- 6 × 32 bytes -----
    // [0] root
    out[12..44].copy_from_slice(root);

    // [1] nullifier_hash
    out[44..76].copy_from_slice(nullifier_hash);

    // [2] recipient — reduced to BN254 field
    let recipient_reduced = reduce_to_field(&recipient.to_bytes());
    out[76..108].copy_from_slice(&recipient_reduced);

    // [3] recipient_binding
    out[108..140].copy_from_slice(recipient_binding);

    // [4] amount — u64 big-endian, right-aligned in a 32-byte word
    //     24 leading zeros, then 8 bytes of u64 BE.
    out[140..164].copy_from_slice(&[0u8; 24]);
    out[164..172].copy_from_slice(&amount.to_be_bytes());

    // [5] total_amount — same shape as amount.
    out[172..196].copy_from_slice(&[0u8; 24]);
    out[196..204].copy_from_slice(&total_amount.to_be_bytes());

    out
}

/// Reduces a 32-byte big-endian value modulo the BN254 scalar field prime.
///
/// The BN254 field is ≈ 2^254, so a 256-bit input can exceed the field.
/// The result is guaranteed to be a valid field element (< BN254_PRIME).
///
/// Algorithm: repeated subtraction of the prime from the input.
///
/// At most 5 iterations are needed:
///   - max input = 2^256 - 1 ≈ 4.006 × p,
///   - so after 5 subtractions, the result is guaranteed to be < p.
///
/// We use 5 (not 4) to handle the edge case where the input is very
/// close to `5 × p`, requiring the full 5 subtractions to fall below `p`.
pub fn reduce_to_field(input: &[u8; 32]) -> [u8; 32] {
    let mut value = *input;

    for _ in 0..5 {
        if !is_ge(&value, &BN254_PRIME_BE) {
            break;
        }
        value = sub_be(&value, &BN254_PRIME_BE);
    }

    value
}

/// Returns true if `a >= b` for two 32-byte big-endian values.
fn is_ge(a: &[u8; 32], b: &[u8; 32]) -> bool {
    for i in 0..32 {
        if a[i] > b[i] {
            return true;
        }
        if a[i] < b[i] {
            return false;
        }
    }
    true // equal
}

/// Subtracts `b` from `a` (a >= b) as 32-byte big-endian values.
/// Returns `a - b`.
fn sub_be(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut borrow: u16 = 0;

    for i in (0..32).rev() {
        let ai = a[i] as u16;
        let bi = b[i] as u16 + borrow;
        if ai >= bi {
            out[i] = (ai - bi) as u8;
            borrow = 0;
        } else {
            out[i] = (ai + 256 - bi) as u8;
            borrow = 1;
        }
    }

    out
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_length_is_204() {
        let out = encode_public_inputs(
            &[1u8; 32],
            &[2u8; 32],
            &Pubkey::new_from_array([3u8; 32]),
            &[4u8; 32],
            300_000,
            1_000_000,
        );
        assert_eq!(out.len(), 204);
    }

    #[test]
    fn test_header() {
        let out = encode_public_inputs(
            &[0u8; 32],
            &[0u8; 32],
            &Pubkey::new_from_array([0u8; 32]),
            &[0u8; 32],
            0,
            0,
        );
        assert_eq!(&out[0..4], &[0, 0, 0, 6]);
        assert_eq!(&out[4..8], &[0, 0, 0, 0]);
        assert_eq!(&out[8..12], &[0, 0, 0, 6]);
    }

    #[test]
    fn test_public_inputs_positions() {
        let root = [0xaau8; 32];
        let nullifier_hash = [0xbbu8; 32];
        let recipient_bytes = [0x11u8; 32];
        let recipient = Pubkey::new_from_array(recipient_bytes);
        let recipient_binding = [0xccu8; 32];
        let amount = 0x123456789abcdef0u64;
        let total_amount = 0xfedcba9876543210u64;

        let out = encode_public_inputs(
            &root,
            &nullifier_hash,
            &recipient,
            &recipient_binding,
            amount,
            total_amount,
        );

        assert_eq!(&out[12..44], &root);
        assert_eq!(&out[44..76], &nullifier_hash);
        let reduced = reduce_to_field(&recipient_bytes);
        assert_eq!(&out[76..108], &reduced);
        assert_eq!(&out[108..140], &recipient_binding);
        assert_eq!(&out[140..164], &[0u8; 24]);
        assert_eq!(&out[164..172], &amount.to_be_bytes());
        assert_eq!(&out[172..196], &[0u8; 24]);
        assert_eq!(&out[196..204], &total_amount.to_be_bytes());
    }

    #[test]
    fn test_amount_and_total_amount_are_distinct() {
        let out = encode_public_inputs(
            &[0u8; 32],
            &[0u8; 32],
            &Pubkey::new_from_array([0u8; 32]),
            &[0u8; 32],
            300_000,
            1_000_000,
        );
        // amount at [164..172]
        assert_eq!(&out[164..172], &300_000u64.to_be_bytes());
        // total_amount at [196..204]
        assert_eq!(&out[196..204], &1_000_000u64.to_be_bytes());
    }

    #[test]
    fn test_reduce_to_field_small_value_unchanged() {
        let v = [0x00u8; 32];
        assert_eq!(reduce_to_field(&v), v);

        let mut v2 = [0u8; 32];
        v2[31] = 0xff;
        assert_eq!(reduce_to_field(&v2), v2);
    }

    #[test]
    fn test_reduce_to_field_prime_is_zero() {
        let reduced = reduce_to_field(&BN254_PRIME_BE);
        assert_eq!(reduced, [0u8; 32]);
    }

    #[test]
    fn test_reduce_to_field_max() {
        let max = [0xffu8; 32];
        let reduced = reduce_to_field(&max);
        assert!(!is_ge(&reduced, &BN254_PRIME_BE));
    }

    #[test]
    fn test_is_ge_equal() {
        let a = [5u8; 32];
        assert!(is_ge(&a, &a));
    }

    #[test]
    fn test_sub_be_simple() {
        let a = {
            let mut v = [0u8; 32];
            v[31] = 10;
            v
        };
        let b = {
            let mut v = [0u8; 32];
            v[31] = 3;
            v
        };
        let c = sub_be(&a, &b);
        let mut expected = [0u8; 32];
        expected[31] = 7;
        assert_eq!(c, expected);
    }

    #[test]
    fn test_sub_be_borrow() {
        let a = {
            let mut v = [0u8; 32];
            v[30] = 1;
            v[31] = 0;
            v
        };
        let b = {
            let mut v = [0u8; 32];
            v[31] = 1;
            v
        };
        let c = sub_be(&a, &b);
        let mut expected = [0u8; 32];
        expected[31] = 255;
        assert_eq!(c, expected);
    }
}
