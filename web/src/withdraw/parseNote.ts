//! Parse a saved deposit note (JSON) into a structured object.
//!
//! The note format matches `DepositForm.vue::copyNote()`:
//! {
//!   "nullifier":     "<bare hex, 64 chars>",
//!   "secret":        "<bare hex, 64 chars>",
//!   "note_secret":   "<bare hex, 64 chars>",
//!   "amount":        "<bare hex, 64 chars>",
//!   "commitment":    "<bare hex, 64 chars>",
//!   "nullifier_hash":"<bare hex, 64 chars>",
//!   "tx_signature":  "<base58>",
//!   "pool_pda":      "<base58>",
//!   "splits":        ["<bare hex>", "<bare hex>", "<bare hex>"],
//!   "note_index":    <0 | 1 | 2>
//! }
//!
//! Stage 15: `splits` and `note_index` are new. Notes created before
//! Stage 15 are **not** compatible — the circuit requires all three
//! split vectors and the note's own index. See `docs/notes/15-split-deposit.md`.
//!
//! Legacy single-commitment notes (created via `deposit`, not
//! `deposit_split`) carry a `splits` array of length 1. Both shapes are
//! accepted here; the withdrawal circuit treats a length-1 vector as
//! `total_amount == amount`.

import { SPLIT_COUNT } from "../constants";

export interface ParsedNote {
  nullifier: string;
  secret: string;
  noteSecret: string;
  amount: string;
  commitment: string;
  nullifierHash: string;
  txSignature: string;
  poolPda: string;
  /**
   * Split vector.
   * - length 1   → legacy single-commitment note (its own amount is total).
   * - length `SPLIT_COUNT` → Stage 15 split note.
   */
  splits: string[];
  /** Index of this note in `splits`. For length-1 vectors, always 0. */
  noteIndex: number;
}

const HEX64_RE = /^[0-9a-f]{64}$/;
const HEX_RE = /^[0-9a-f]+$/;

function requireHex64(v: unknown, name: string): string {
  if (typeof v !== "string" || !HEX64_RE.test(v)) {
    throw new Error(`note.${name} must be a 64-char bare hex string`);
  }
  return v;
}

/** Accepts any non-empty bare-hex string (leading zeros elided). */
function requireHexFlexible(v: unknown, name: string): string {
  if (typeof v !== "string" || v.length === 0 || v.length > 64 || !HEX_RE.test(v)) {
    throw new Error(`note.${name} must be a bare hex string (up to 64 chars)`);
  }
  return v;
}

function requireString(v: unknown, name: string): string {
  if (typeof v !== "string" || v.length === 0) {
    throw new Error(`note.${name} must be a non-empty string`);
  }
  return v;
}

/**
 * Parse JSON text (or a pre-parsed object) into a validated `ParsedNote`.
 */
export function parseNote(input: string | Record<string, unknown>): ParsedNote {
  let raw: Record<string, unknown>;
  if (typeof input === "string") {
    try {
      raw = JSON.parse(input);
    } catch (e) {
      throw new Error(`note is not valid JSON: ${e instanceof Error ? e.message : String(e)}`);
    }
  } else {
    raw = input;
  }

  // ---- splits ----
  if (!Array.isArray(raw.splits)) {
    throw new Error(`note.splits must be an array of 1 or ${SPLIT_COUNT} hex strings`);
  }
  if (raw.splits.length !== 1 && raw.splits.length !== SPLIT_COUNT) {
    throw new Error(`note.splits must have 1 or ${SPLIT_COUNT} elements, got ${raw.splits.length}`);
  }
  const splits: string[] = raw.splits.map((s, i) => requireHexFlexible(s, `splits[${i}]`));

  // ---- note_index ----
  if (typeof raw.note_index !== "number" || !Number.isInteger(raw.note_index)) {
    throw new Error(`note.note_index must be an integer`);
  }
  if (raw.note_index < 0 || raw.note_index >= splits.length) {
    throw new Error(`note.note_index must be in [0, ${splits.length})`);
  }

  return {
    nullifier: requireHex64(raw.nullifier, "nullifier"),
    secret: requireHex64(raw.secret, "secret"),
    noteSecret: requireHex64(raw.note_secret, "note_secret"),
    amount: requireHex64(raw.amount, "amount"),
    commitment: requireHex64(raw.commitment, "commitment"),
    nullifierHash: requireHex64(raw.nullifier_hash, "nullifier_hash"),
    txSignature: requireString(raw.tx_signature, "tx_signature"),
    poolPda: requireString(raw.pool_pda, "pool_pda"),
    splits,
    noteIndex: raw.note_index,
  };
}
