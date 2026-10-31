//! Tests for poseidon2Hash (bare-hex convention).

import { test } from "node:test";
import assert from "node:assert/strict";
import { poseidon2Hash } from "../src/poseidon.js";

const ZERO = "00".repeat(32);
const ONE = "0".repeat(63) + "1";
const TWO = "0".repeat(63) + "2";

test("poseidon2Hash(1, 2) matches known value", async () => {
  const h = await poseidon2Hash(ONE, TWO);
  assert.equal(h, "299bfccd7daf3c917e51291383929049ec0eaed800af245056cbf135f7dea636");
});

test("poseidon2Hash is deterministic", async () => {
  const a = await poseidon2Hash(ONE, TWO);
  const b = await poseidon2Hash(ONE, TWO);
  assert.equal(a, b);
});

test("poseidon2Hash is not commutative", async () => {
  const ab = await poseidon2Hash(ONE, TWO);
  const ba = await poseidon2Hash(TWO, ONE);
  assert.notEqual(ab, ba);
});

test("poseidon2Hash output is 64-char bare hex", async () => {
  const h = await poseidon2Hash(ZERO, ZERO);
  assert.match(h, /^[0-9a-f]{64}$/);
});
