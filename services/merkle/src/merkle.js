//! Merkle tree operations matching the on-chain and backend conventions.
//!
//! Conventions (must match `services/backend/src/tree.rs`):
//!   - empty[0]   = "00".repeat(32)  (32 zero bytes, bare hex)
//!   - empty[d+1] = hash_2(empty[d], empty[d])
//!   - A leaf at index `i` at level `d` is the LEFT child iff bit `d` of `i` is 0.
//!   - Root is at level DEPTH (20).
//!
//! All inputs and outputs are BARE hex strings (no `0x` prefix),
//! matching services/backend/src/tree.rs.
//!
//! Key indexing note:
//!   `empty[d]` is the hash of an empty subtree with 2^d leaves.
//!   At tree level `d` (leaves at d=0), an unpair sibling covers 2^(DEPTH - d - 1)
//!   leaves, so the sibling hash is `empty[DEPTH - d - 1]`.
//!
//! API:
//!   computeRoot(commitments: string[]) -> Promise<string>
//!   computeProof(commitments: string[], leafIndex: number)
//!     -> Promise<{proof: string[], isEven: boolean[]}>

import { poseidon2Hash } from "./poseidon.js";

export const TREE_DEPTH = 20;

const ZERO_LEAF = "00".repeat(32);

/**
 * Compute the cascade of empty hashes.
 *   empty[0]   = ZERO_LEAF
 *   empty[d+1] = hash_2(empty[d], empty[d])
 *
 * @returns {Promise<string[]>} array of length TREE_DEPTH + 1
 */
export async function computeEmptyHashes() {
  const empty = [ZERO_LEAF];
  for (let d = 0; d < TREE_DEPTH; d++) {
    const h = await poseidon2Hash(empty[d], empty[d]);
    empty.push(h);
  }
  return empty;
}

/**
 * Compute the Merkle root from a list of commitments.
 * Missing leaves (up to 2^TREE_DEPTH) are treated as empty subtrees.
 *
 * @param {string[]} commitments - bare hex strings, in insertion order
 * @returns {Promise<string>} root (bare hex)
 */
export async function computeRoot(commitments) {
  if (commitments.length > 2 ** TREE_DEPTH) {
    throw new Error(`too many commitments: ${commitments.length} > 2^${TREE_DEPTH}`);
  }

  const empty = await computeEmptyHashes();

  if (commitments.length === 0) {
    return empty[TREE_DEPTH];
  }

  let level = commitments.slice();
  let currentLevel = 0;

  while (currentLevel < TREE_DEPTH) {
    const next = [];
    // Sibling for an unpair node at this level covers
    // 2^(TREE_DEPTH - currentLevel - 1) leaves.
    const emptySibling = empty[TREE_DEPTH - currentLevel - 1];

    for (let i = 0; i < level.length; i += 2) {
      const left = level[i];
      const right = i + 1 < level.length ? level[i + 1] : emptySibling;
      next.push(await poseidon2Hash(left, right));
    }

    level = next;
    currentLevel += 1;

    if (level.length === 1 && currentLevel === TREE_DEPTH) {
      return level[0];
    }

    // Single real node still needs to be paired with empty siblings up to the root.
    if (level.length === 1) {
      let cur = level[0];
      for (let d = currentLevel; d < TREE_DEPTH; d++) {
        const sibling = empty[TREE_DEPTH - d - 1];
        cur = await poseidon2Hash(cur, sibling);
      }
      return cur;
    }
  }

  throw new Error(
    `internal error: no root reached (level=${currentLevel}, length=${level.length})`,
  );
}

/**
 * Compute a Merkle proof for a leaf at `leafIndex`.
 *
 * @param {string[]} commitments - bare hex strings
 * @param {number} leafIndex
 * @returns {Promise<{proof: string[], isEven: boolean[]}>}
 *   proof[d]  — sibling hash at level d
 *   isEven[d] — true iff our node is the LEFT child at level d
 */
export async function computeProof(commitments, leafIndex) {
  if (leafIndex < 0 || leafIndex >= commitments.length) {
    throw new Error(`leafIndex ${leafIndex} out of range [0, ${commitments.length})`);
  }

  const empty = await computeEmptyHashes();
  const proof = [];
  const isEven = [];

  let level = commitments.slice();
  let idx = leafIndex;

  for (let d = 0; d < TREE_DEPTH; d++) {
    const isLeft = (idx & 1) === 0;
    const siblingIdx = isLeft ? idx + 1 : idx - 1;

    // Sibling at level d covers 2^(TREE_DEPTH - d - 1) leaves if empty.
    const emptySibling = empty[TREE_DEPTH - d - 1];

    let sibling;
    if (siblingIdx >= 0 && siblingIdx < level.length) {
      sibling = level[siblingIdx];
    } else {
      sibling = emptySibling;
    }

    proof.push(sibling);
    isEven.push(isLeft);

    // Build the next level.
    const next = [];
    for (let i = 0; i < level.length; i += 2) {
      const left = level[i];
      const right = i + 1 < level.length ? level[i + 1] : emptySibling;
      next.push(await poseidon2Hash(left, right));
    }
    level = next;
    idx = idx >> 1;

    if (level.length === 0) {
      // No more real nodes; remaining siblings are all empty, and since our
      // node is the only real one left, it is always the LEFT child.
      for (let dd = d + 1; dd < TREE_DEPTH; dd++) {
        proof.push(empty[TREE_DEPTH - dd - 1]);
        isEven.push(true);
      }
      break;
    }
  }

  return { proof, isEven };
}
