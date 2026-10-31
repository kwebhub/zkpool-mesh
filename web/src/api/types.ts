//! TypeScript types for the backend HTTP API.
//!
//! All types mirror `services/backend/src/main.rs` responses.
//!
//! Stage 15: the circuit now has 6 public inputs (was 5) and 7 private
//! inputs (was 5). The wire format mirrors `services/backend/src/api_types.rs`.

/** Bare-hex string (no `0x` prefix). */
export type Hex = string;

/** Base64-encoded bytes. */
export type Base64 = string;

// ---- /api/health ----

export interface HealthResponse {
  status: "ok" | "degraded";
  db: boolean;
  version: string;
}

// ---- /api/commitments ----

export interface CommitmentRecord {
  id: number;
  leaf_index: number;
  /** Bare hex, 64 chars. */
  commitment: Hex;
  pool_address: string;
  /** Transaction signature; may be null for synthetic rows. */
  tx_signature: string | null;
  /** ISO-8601 timestamp. */
  created_at: string;
}

export interface CommitmentsResponse {
  pool_address: string;
  count: number;
  commitments: CommitmentRecord[];
}

// ---- /api/root ----

export interface RootResponse {
  pool_address: string;
  /** Bare hex, 64 chars; `null` if no root yet. */
  root: Hex | null;
}

// ---- /api/proof ----

export interface ProofResponse {
  pool_address: string;
  leaf_index: number;
  /** 20 sibling hashes, bare hex. */
  proof: Hex[];
  /** 20 side flags. true = our node is the left child. */
  is_even: boolean[];
}

// ---- /api/withdraw ----

/**
 * Request body for `POST /api/withdraw`.
 *
 * Field names and encoding match `services/backend/src/api_types.rs`.
 * Stage 15 additions:
 *   - `total_amount` (public, aggregate deposit amount)
 *   - `splits` (private, 3 field elements)
 *   - `note_index` (private, u32 in [0, 3))
 */
export interface WithdrawRequest {
  // Public (6)
  root: Hex;
  nullifier_hash: Hex;
  recipient: Hex;
  recipient_binding: Hex;
  amount: Hex;
  total_amount: Hex;
  // Private (7)
  nullifier: Hex;
  secret: Hex;
  note_secret: Hex;
  merkle_proof: Hex[];
  is_even: boolean[];
  splits: Hex[];
  note_index: number;
}

export interface WithdrawResponse {
  /** Base64-encoded Groth16 proof (388 bytes since Stage 15.4). */
  proof: Base64;
  /** Base64-encoded public witness (204 bytes since Stage 15.4). */
  public_witness: Base64;
}

// ---- Errors ----

export interface ApiError {
  error: string;
  [key: string]: unknown;
}
