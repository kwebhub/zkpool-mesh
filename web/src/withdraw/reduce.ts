//! Reduce a 32-byte big-endian value modulo the BN254 scalar field prime.
//!
//! Port of `onchain/programs/zk_pool/src/encoding.rs::reduce_to_field`.
//! Must produce byte-identical results — verified against the Rust version
//! via the on-chain test suite + this helper's own test.
//!
//! The BN254 field is ≈ 2^254, so a 256-bit input can exceed it at most
//! 4 times. We do 5 subtractions to be safe (matches the Rust implementation).

/** BN254 scalar field prime, big-endian 32 bytes. */
const BN254_PRIME_BE: Uint8Array<ArrayBuffer> = new Uint8Array([
  0x30, 0x64, 0x4e, 0x72, 0xe1, 0x31, 0xa0, 0x29, 0xb8, 0x50, 0x45, 0xb6, 0x81, 0x81, 0x58, 0x5d,
  0x28, 0x33, 0xe8, 0x48, 0x79, 0xb9, 0x70, 0x91, 0x43, 0xe1, 0xf5, 0x93, 0xf0, 0x00, 0x00, 0x01,
]);

/** Returns true if `a >= b` (both 32-byte big-endian). */
function isGe(a: Uint8Array, b: Uint8Array): boolean {
  for (let i = 0; i < 32; i++) {
    if (a[i] > b[i]) return true;
    if (a[i] < b[i]) return false;
  }
  return true;
}

/** Returns `a - b` (assumes `a >= b`), 32-byte big-endian. */
function subBe(a: Uint8Array, b: Uint8Array): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(32);
  let borrow = 0;
  for (let i = 31; i >= 0; i--) {
    const ai = a[i];
    const bi = b[i] + borrow;
    if (ai >= bi) {
      out[i] = ai - bi;
      borrow = 0;
    } else {
      out[i] = ai + 256 - bi;
      borrow = 1;
    }
  }
  return out;
}

/**
 * Reduce a 32-byte big-endian value modulo the BN254 scalar field prime.
 *
 * @param input 32 bytes
 * @returns 32 bytes, guaranteed < prime
 */
export function reduceToField(input: Uint8Array): Uint8Array<ArrayBuffer> {
  if (input.length !== 32) {
    throw new Error(`reduceToField expects 32 bytes, got ${input.length}`);
  }
  let value: Uint8Array<ArrayBuffer> = new Uint8Array(input);
  for (let i = 0; i < 5; i++) {
    if (!isGe(value, BN254_PRIME_BE)) break;
    value = subBe(value, BN254_PRIME_BE);
  }
  return value;
}

/** Bare hex representation (64 chars) of the reduced value. */
export function reduceToFieldHex(input: Uint8Array): string {
  return bytesToHex(reduceToField(input));
}

export function bytesToHex(bytes: Uint8Array): string {
  let out = "";
  for (const b of bytes) {
    out += b.toString(16).padStart(2, "0");
  }
  return out;
}

export function hexToBytes(hex: string): Uint8Array<ArrayBuffer> {
  if (hex.length % 2 !== 0) throw new Error(`odd hex length: ${hex.length}`);
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}
