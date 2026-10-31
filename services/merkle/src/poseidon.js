//! Poseidon2 hashing via @noir-lang/noir_js.
//!
//! Why this file exists:
//!   Rust and JS Poseidon2 implementations produce DIFFERENT hashes than the
//!   Noir circuit's builtin. To guarantee byte-identical results, we load the
//!   SAME ACIR (hash2.json) that the withdrawal circuit uses, and execute it
//!   via noir_js. This is the only way to guarantee identity across layers.
//!
//! Convention (matches services/backend/src/tree.rs):
//!   All inputs and outputs are BARE hex strings (no `0x` prefix),
//!   64 hex digits = 32 bytes = one BN254 field element.
//!   Internally we add `0x` for noir_js and strip it from the result.

import { Noir } from "@noir-lang/noir_js";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const CIRCUIT_PATH = join(__dirname, "..", "circuits", "hash2.json");

// Load circuit once at module init.
const circuit = JSON.parse(readFileSync(CIRCUIT_PATH, "utf8"));
const noir = new Noir(circuit);

/**
 * Compute Poseidon2 hash of two field elements.
 *
 * @param {string} left  - bare hex (64 chars, no 0x)
 * @param {string} right - bare hex (64 chars, no 0x)
 * @returns {Promise<string>} bare hex (64 chars, no 0x)
 */
export async function poseidon2Hash(left, right) {
  const result = await noir.execute({
    left: "0x" + left,
    right: "0x" + right,
  });
  // returnValue is a 0x-prefixed hex string; strip the prefix.
  return result.returnValue.slice(2);
}
