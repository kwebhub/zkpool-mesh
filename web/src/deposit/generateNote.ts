//! Generate deposit notes for a 3-way split deposit.
//!
//! Stage 15: `deposit_split` creates `SPLIT_COUNT` commitments in a single
//! transaction. Each commitment has its own secrets and its own amount; the
//! sums must equal the aggregate `total_amount` (enforced by the circuit's
//! C4 constraint and by `deposit_split` on-chain).
//!
//! ## Note format
//!
//! Each individual note carries the shared split vector and its own index:
//! {
//!   nullifier, secret, noteSecret,
//!   amount,               // this note's amount (hex, 64 chars)
//!   commitment, nullifierHash,
//!   splits: [hex, hex, hex],  // ALL three amounts, in order
//!   noteIndex: 0 | 1 | 2,
//!   txSignature, poolPda,
//! }
//!
//! The user must save all three notes; losing any one loses those funds.
//! See `docs/notes/15-split-deposit.md`.

import { computeHashes } from "../noir/hashes";
import { SPLIT_COUNT } from "../constants";

export interface DepositNote {
  /** Random field element, hashed into nullifier_hash. */
  nullifier: string;
  /** Random field element, part of the commitment. */
  secret: string;
  /** Random field element, part of recipient_binding. */
  noteSecret: string;
  /** This note's amount (lamports, as hex, 64 chars). */
  amount: string;
  /** Derived commitment = hash_3(nullifier, secret, amount). Bare hex. */
  commitment: string;
  /** Derived nullifier_hash = hash_1(nullifier). Bare hex. */
  nullifierHash: string;
  /** Split vector — all SPLIT_COUNT amounts, in order (hex, 64 chars each). */
  splits: string[];
  /** Index of this note in `splits`. */
  noteIndex: number;
}

/**
 * Generate a random field element as bare hex (64 chars).
 *
 * Uses Web Crypto API. Top byte is masked to keep the value comfortably
 * below the BN254 field prime (2^254), matching the Noir circuit's
 * expectation.
 */
function randomFieldHex(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  // Clear the top 3 bits — the BN254 prime begins with 0x30, so keeping
  // the top byte ≤ 0x1f guarantees the value is below the modulus.
  bytes[0] &= 0x1f;
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/** Convert a lamport amount to a 64-char padded bare-hex string. */
function lamportsToHex(lamports: bigint): string {
  return lamports.toString(16).padStart(64, "0");
}

/**
 * Generate a single deposit note for the given amount (lamports).
 *
 * Legacy single-commitment path (see Stage 9 `deposit`). Kept for
 * backwards-compatible callers; new code should use `generateSplitNotes`.
 */
export async function generateNote(amountLamports: bigint): Promise<DepositNote> {
  const nullifier = randomFieldHex();
  const secret = randomFieldHex();
  const noteSecret = randomFieldHex();
  const amount = lamportsToHex(amountLamports);

  const { commitment, nullifierHash } = await computeHashes(nullifier, secret, amount);

  return {
    nullifier,
    secret,
    noteSecret,
    amount,
    commitment,
    nullifierHash,
    // Single-element split, treated as a one-element array. The circuit
    // rejects this — SPLIT_COUNT = 3 — but keeping the shape consistent
    // makes the interface uniform.
    splits: [amount],
    noteIndex: 0,
  };
}

/**
 * Generate `SPLIT_COUNT` notes for a split deposit.
 *
 * @param amounts  exactly `SPLIT_COUNT` per-note amounts, in lamports
 * @returns the notes, in the same order as `amounts`
 *
 * `Σ amounts` is not passed explicitly — the caller derives it and sends
 * it as `total_amount` to the on-chain `deposit_split`.
 */
export async function generateSplitNotes(amounts: bigint[]): Promise<DepositNote[]> {
  if (amounts.length !== SPLIT_COUNT) {
    throw new Error(`expected ${SPLIT_COUNT} amounts, got ${amounts.length}`);
  }

  // Precompute hex split vector — the same array is stored in every note.
  const splitsHex = amounts.map(lamportsToHex);

  const notes: DepositNote[] = [];
  for (let i = 0; i < SPLIT_COUNT; i++) {
    const nullifier = randomFieldHex();
    const secret = randomFieldHex();
    const noteSecret = randomFieldHex();
    const amount = splitsHex[i];

    const { commitment, nullifierHash } = await computeHashes(nullifier, secret, amount);

    notes.push({
      nullifier,
      secret,
      noteSecret,
      amount,
      commitment,
      nullifierHash,
      splits: splitsHex,
      noteIndex: i,
    });
  }

  return notes;
}
