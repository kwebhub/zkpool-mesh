//! Poseidon2 hashing via `@noir-lang/noir_js`, running in the browser.
//!
//! Uses the SAME ACIR (`/circuits/hash2.json`) as the Merkle service and
//! the withdrawal circuit. This guarantees byte-identical hashes across
//! all layers — the only way to avoid the "mismatch between layers" bug
//! class that killed v2.
//!
//! Convention: bare hex (no `0x`) at the boundary, matching the backend,
//! the Merkle service, and the prover. Internally `0x` is added for
//! `noir_js` and stripped from the result.
//!
//! Stage 16.1: `@noir-lang/noir_js` is loaded via dynamic `import()` so
//! that its ~3.84 MB WASM (acvm_js + noirc_abi_wasm) is only pulled in
//! when a proof-relevant action starts — not on initial page load.

import type { Noir } from "@noir-lang/noir_js";

let cachedNoir: Noir | null = null;

async function loadHash2(): Promise<Noir> {
  if (cachedNoir) return cachedNoir;
  const [noirMod, resp] = await Promise.all([
    import("@noir-lang/noir_js"),
    fetch("/circuits/hash2.json"),
  ]);
  if (!resp.ok) {
    throw new Error(`failed to load hash2.json: HTTP ${resp.status}`);
  }
  const circuit = await resp.json();
  cachedNoir = new noirMod.Noir(circuit);
  return cachedNoir;
}

/**
 * Compute Poseidon2 hash of two field elements.
 *
 * @param left  bare hex (any length, will be accepted by noir_js)
 * @param right bare hex
 * @returns bare hex (66 chars typically; leading zeros preserved by noir_js)
 */
export async function poseidon2Hash(left: string, right: string): Promise<string> {
  const noir = await loadHash2();
  const result = await noir.execute({
    left: "0x" + left,
    right: "0x" + right,
  });
  // returnValue is a 0x-prefixed hex string; strip the prefix.
  return (result.returnValue as string).slice(2);
}
