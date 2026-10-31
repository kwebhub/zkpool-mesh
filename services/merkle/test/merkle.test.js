//! Tests for Merkle tree: empty hashes, root, proof, circuit replay.

import { test } from "node:test";
import assert from "node:assert/strict";
import { poseidon2Hash } from "../src/poseidon.js";
import { computeEmptyHashes, computeRoot, computeProof, TREE_DEPTH } from "../src/merkle.js";

const ZERO = "00".repeat(32);
const COMMITMENT = "09d9d188784ab20199a5eb7267ce27765a374ce6bc672deee9eeac9ba90b80fc";

test("empty[0] is 32 zero bytes", async () => {
  const empty = await computeEmptyHashes();
  assert.equal(empty[0], ZERO);
});

test("empty has cascade length TREE_DEPTH + 1", async () => {
  const empty = await computeEmptyHashes();
  assert.equal(empty.length, TREE_DEPTH + 1);
});

test("empty[d+1] = hash_2(empty[d], empty[d])", async () => {
  const empty = await computeEmptyHashes();
  const recomputed = await poseidon2Hash(empty[0], empty[0]);
  assert.equal(empty[1], recomputed);
});

test("computeRoot([]) returns empty[TREE_DEPTH]", async () => {
  const empty = await computeEmptyHashes();
  const root = await computeRoot([]);
  assert.equal(root, empty[TREE_DEPTH]);
});

test("computeRoot single leaf matches manual fold", async () => {
  const empty = await computeEmptyHashes();
  let cur = COMMITMENT;
  for (let d = 0; d < TREE_DEPTH; d++) {
    cur = await poseidon2Hash(cur, empty[TREE_DEPTH - d - 1]);
  }
  const root = await computeRoot([COMMITMENT]);
  assert.equal(root, cur);
});

test("computeProof single leaf replays to same root", async () => {
  const root = await computeRoot([COMMITMENT]);
  const { proof, isEven } = await computeProof([COMMITMENT], 0);

  let cur = COMMITMENT;
  for (let i = 0; i < TREE_DEPTH; i++) {
    const sibling = proof[i];
    const [left, right] = isEven[i] ? [cur, sibling] : [sibling, cur];
    cur = await poseidon2Hash(left, right);
  }
  assert.equal(cur, root);
});

test("computeProof single leaf: proof and isEven length 20, all left", async () => {
  const { proof, isEven } = await computeProof([COMMITMENT], 0);
  assert.equal(proof.length, TREE_DEPTH);
  assert.equal(isEven.length, TREE_DEPTH);
  assert.ok(isEven.every((x) => x === true));
});

test("computeProof multi-leaf: all leaves replay to same root", async () => {
  const leaves = [COMMITMENT, "1".repeat(64), "2".repeat(64)];
  const root = await computeRoot(leaves);
  for (let i = 0; i < leaves.length; i++) {
    const { proof, isEven } = await computeProof(leaves, i);
    let cur = leaves[i];
    for (let d = 0; d < TREE_DEPTH; d++) {
      const sibling = proof[d];
      const [left, right] = isEven[d] ? [cur, sibling] : [sibling, cur];
      cur = await poseidon2Hash(left, right);
    }
    assert.equal(cur, root, `leaf ${i} did not replay to root`);
  }
});

test("computeProof rejects out-of-range index", async () => {
  await assert.rejects(() => computeProof([COMMITMENT], 5), /out of range/);
});

test("computeRoot too many commitments throws", async () => {
  const tooMany = new Array(2 ** TREE_DEPTH + 1).fill(COMMITMENT);
  await assert.rejects(() => computeRoot(tooMany), /too many/);
});
