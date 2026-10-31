//! Commitment + nullifier_hash via `@noir-lang/noir_js`.
//!
//! Uses `/circuits/hashes.json` — the same ACIR exposed as `hashes` in
//! Stage 2. Inputs: `(nullifier, secret, amount)`. Output: a tuple
//! `(commitment, nullifier_hash)` as a pair of `0x`-prefixed hex strings.
//!
//! Stage 16.1: `@noir-lang/noir_js` is loaded via dynamic `import()` so
//! that its ~3.84 MB WASM (acvm_js + noirc_abi_wasm) is only pulled in
//! when a proof-relevant action starts — not on initial page load.

import type { Noir } from "@noir-lang/noir_js";

let cachedNoir: Noir | null = null;

async function loadHashes(): Promise<Noir> {
  if (cachedNoir) return cachedNoir;
  const [noirMod, resp] = await Promise.all([
    import("@noir-lang/noir_js"),
    fetch("/circuits/hashes.json"),
  ]);
  if (!resp.ok) {
    throw new Error(`failed to load hashes.json: HTTP ${resp.status}`);
  }
  const circuit = await resp.json();
  cachedNoir = new noirMod.Noir(circuit);
  return cachedNoir;
}

export interface CommitmentResult {
  /** Commitment (bare hex, no `0x`). */
  commitment: string;
  /** Nullifier hash (bare hex, no `0x`). */
  nullifierHash: string;
}

/**
 * Compute `commitment = hash_3(nullifier, secret, amount)` and
 * `nullifier_hash = hash_1(nullifier)`.
 */
export async function computeHashes(
  nullifier: string,
  secret: string,
  amount: string,
): Promise<CommitmentResult> {
  const noir = await loadHashes();
  const result = await noir.execute({
    nullifier: "0x" + nullifier,
    secret: "0x" + secret,
    amount: "0x" + amount,
  });
  const [commitment, nullifierHash] = result.returnValue as [string, string];
  return {
    commitment: commitment.slice(2),
    nullifierHash: nullifierHash.slice(2),
  };
}
