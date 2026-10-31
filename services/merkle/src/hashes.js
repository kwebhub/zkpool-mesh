//! Commitment + nullifier_hash via the `hashes` ACIR.
//!
//! Wraps `@noir-lang/noir_js` with a cached `Noir` instance, mirroring the
//! pattern in `poseidon.js`. Used by:
//!   - the frontend (its own copy in `web/src/noir/hashes.ts`)
//!   - Rust E2E scripts via `POST /hashes`
//!
//! Inputs:  `nullifier`, `secret`, `amount` (bare hex, no `0x`).
//! Outputs: `{ commitment, nullifier_hash }` (bare hex).

import { Noir } from "@noir-lang/noir_js";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const CIRCUIT_PATH = join(__dirname, "..", "circuits", "hashes.json");

const circuit = JSON.parse(readFileSync(CIRCUIT_PATH, "utf8"));
const noir = new Noir(circuit);

/**
 * Compute `commitment = hash_3(nullifier, secret, amount)` and
 * `nullifier_hash = hash_1(nullifier)`.
 *
 * @param {string} nullifier - bare hex
 * @param {string} secret    - bare hex
 * @param {string} amount    - bare hex (lamports, padded to 32 bytes)
 * @returns {Promise<{commitment: string, nullifier_hash: string}>}
 */
export async function computeHashes(nullifier, secret, amount) {
  const result = await noir.execute({
    nullifier: "0x" + nullifier,
    secret: "0x" + secret,
    amount: "0x" + amount,
  });
  const [commitment, nullifierHash] = result.returnValue;
  return {
    commitment: commitment.slice(2),
    nullifier_hash: nullifierHash.slice(2),
  };
}
