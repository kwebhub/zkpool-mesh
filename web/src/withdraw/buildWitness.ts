//! Assemble a full withdrawal witness and obtain a Groth16 proof.
//!
//! Steps:
//!   1. Compute `nullifier_hash` = hash_1(nullifier) — browser noir_js.
//!   2. Reduce recipient address to a BN254 field element.
//!   3. Compute `recipient_binding` = hash_2(note_secret, recipient_reduced).
//!   4. Look up the leaf index for `commitment` in `/api/commitments`.
//!   5. Fetch the Merkle proof from `/api/proof`.
//!   6. Call `/api/withdraw` → Groth16 proof + public witness (base64).
//!
//! Returns everything needed to build the on-chain `withdraw` instruction.
//!
//! Stage 15: witness now carries `total_amount` (public), `splits[3]` and
//! `note_index` (private). Proof is 388 B, public witness 204 B.
//!
//! Legacy single-commitment notes (created via `deposit`, not
//! `deposit_split`) carry a `splits` array of length 1. The circuit
//! hard-codes `splits: [Field; 3]`, so we pad a length-1 vector to
//! `[amount, 0, 0]` with `note_index = 0`. Both constraints C4
//! (`Σ splits[i] == total_amount`) and C5 (`splits[note_index] == amount`)
//! then hold.

import { address, type Address } from "@solana/kit";
import { getCommitments, getProof, getRoot, postWithdraw } from "../api/client";
import { poseidon2Hash } from "../noir/poseidon";
import { reduceToFieldHex } from "./reduce";
import type { ParsedNote } from "./parseNote";
import { SPLIT_COUNT } from "../constants";

export interface WithdrawWitness {
  /** Public inputs (bare hex, 64 chars each). */
  root: string;
  nullifierHash: string;
  /** Recipient address (base58, original, unreduced). */
  recipient: Address;
  /** Recipient as BN254 field element (bare hex, 64 chars). */
  recipientFieldHex: string;
  recipientBinding: string;
  /** This note's amount in lamports (decimal string). */
  amount: string;
  /** Aggregate deposit amount in lamports (decimal string). */
  totalAmount: string;
  /** Split vector, padded to length `SPLIT_COUNT` (bare-hex strings). */
  splits: string[];
  /** Index of this note in `splits` (0 for legacy single notes). */
  noteIndex: number;
  /** Merkle proof (20 hex strings, bare). */
  merkleProof: string[];
  /** Side flags (20 booleans). */
  isEven: boolean[];
  /** Groth16 proof (base64, 388 bytes decoded). */
  proofBase64: string;
  /** Public witness (base64, 204 bytes decoded). */
  publicWitnessBase64: string;
}

export async function buildWitness(
  note: ParsedNote,
  recipientBase58: string,
): Promise<WithdrawWitness> {
  // 1. nullifier_hash — recompute from note (may differ from saved if
  //    the note was hand-copied; always trust our own computation).
  const nullifierHash = await poseidon2Hash(
    note.nullifier,
    "0".repeat(64), // hash_1(x) == hash_2(x, 0) by zero-padding
  );

  // 2. Recipient reduction.
  const recipientAddr = address(recipientBase58);
  const recipientBytes = base58ToBytes(recipientBase58);
  const recipientFieldHex = reduceToFieldHex(recipientBytes);

  // 3. recipient_binding = hash_2(note_secret, recipient_field)
  const recipientBinding = await poseidon2Hash(note.noteSecret, recipientFieldHex);

  // 4. Locate leaf index by commitment.
  const { commitments } = await getCommitments();
  const leafIndex = commitments.findIndex((c) => c.commitment === note.commitment);
  if (leafIndex < 0) {
    throw new Error("commitment not found in the pool — has it been indexed yet?");
  }

  // 5. Merkle proof.
  const { proof, is_even } = await getProof(leafIndex);

  // 5b. Current root from the backend.
  const { root: rootOrNull } = await getRoot();
  if (!rootOrNull) {
    throw new Error("no root available — is the pool initialized and indexed?");
  }
  const root = rootOrNull;

  // 6. The circuit hard-codes `splits` as `[Field; 3]`. A legacy
  //    single-commitment note (length-1 vector) is padded with two
  //    zeros: `[amount, 0, 0]`, `note_index = 0`, and
  //    `total_amount = amount`. C4 (`Σ splits[i] == total_amount`) and
  //    C5 (`splits[note_index] == amount`) both hold.
  const splitsPadded: string[] =
    note.splits.length === SPLIT_COUNT
      ? note.splits
      : [note.amount, "0".repeat(64), "0".repeat(64)];
  const noteIndexPadded: number = note.splits.length === SPLIT_COUNT ? note.noteIndex : 0;
  const totalBn = splitsPadded.reduce((acc, s) => acc + BigInt("0x" + s), 0n);
  const totalAmountHex = totalBn.toString(16).padStart(64, "0");

  // 7. Groth16 proof via backend → prover.
  const { proof: proofBase64, public_witness: publicWitnessBase64 } = await postWithdraw({
    root,
    nullifier_hash: nullifierHash,
    recipient: recipientFieldHex,
    recipient_binding: recipientBinding,
    amount: note.amount,
    total_amount: totalAmountHex,
    nullifier: note.nullifier,
    secret: note.secret,
    note_secret: note.noteSecret,
    merkle_proof: proof,
    is_even,
    splits: splitsPadded,
    note_index: noteIndexPadded,
  });

  return {
    root,
    nullifierHash,
    recipient: recipientAddr,
    recipientFieldHex,
    recipientBinding,
    amount: BigInt("0x" + note.amount).toString(),
    totalAmount: totalBn.toString(),
    splits: splitsPadded,
    noteIndex: noteIndexPadded,
    merkleProof: proof,
    isEven: is_even,
    proofBase64,
    publicWitnessBase64,
  };
}

/**
 * Decode a base58 string (Solana address) to 32 bytes.
 *
 * Minimal base58 decoder — sufficient for Solana pubkeys.
 */
function base58ToBytes(s: string): Uint8Array {
  const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
  const bytes: number[] = [0];
  for (const c of s) {
    const idx = ALPHABET.indexOf(c);
    if (idx < 0) throw new Error(`invalid base58 character: ${c}`);
    let carry = idx;
    for (let j = 0; j < bytes.length; j++) {
      carry += bytes[j] * 58;
      bytes[j] = carry & 0xff;
      carry >>= 8;
    }
    while (carry > 0) {
      bytes.push(carry & 0xff);
      carry >>= 8;
    }
  }
  // Leading '1's → leading zeros.
  for (const c of s) {
    if (c !== "1") break;
    bytes.push(0);
  }
  // Reverse and pad to 32 bytes.
  const reversed = bytes.reverse();
  const out = new Uint8Array(32);
  out.set(reversed, 32 - reversed.length);
  return out;
}
