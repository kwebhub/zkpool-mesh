//! Tests for reduceToField — must match Rust `encoding.rs::tests`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { reduceToField, reduceToFieldHex, hexToBytes, bytesToHex } from "./reduce.ts";

const PRIME_HEX = "30644e72e131a029b85045b68181585d2833e84879b9709143e1f593f0000001";

test("zero stays zero", () => {
  const zero = new Uint8Array(32);
  assert.equal(bytesToHex(reduceToField(zero)), "0".repeat(64));
});

test("prime reduces to zero", () => {
  const prime = hexToBytes(PRIME_HEX);
  assert.equal(bytesToHex(reduceToField(prime)), "0".repeat(64));
});

test("max value reduces below prime", () => {
  const max = new Uint8Array(32).fill(0xff);
  const reduced = reduceToField(max);
  const reducedHex = bytesToHex(reduced);
  // Must be < prime — compare lexicographically.
  assert.ok(reducedHex < PRIME_HEX, `reduced ${reducedHex} not < prime`);
});

test("small value unchanged", () => {
  const v = new Uint8Array(32);
  v[31] = 0xff;
  assert.equal(reduceToFieldHex(v), "0".repeat(62) + "ff");
});
