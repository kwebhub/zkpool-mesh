//! Frontend constants — must match `spec.json` and on-chain constants.
//!
//! Sync via `scripts/validate-spec` (CI, Stage 12).

import { address, type Address } from "@solana/kit";

/** `zk_pool` program ID (devnet). */
export const ZK_POOL_PROGRAM_ID: Address = address("8cGzkFK9H15mcpndAaY7ApCJhkHcujttR4E2D8rS6LCm");

/** Verifier program ID (devnet) — used by `withdraw` CPI. */
export const VERIFIER_PROGRAM_ID: Address = address("5t51iu6apRxgLbt91eVZ6YYzHsnmBCVnLGqJtqdfMFWJ");

/** Pool PDA — derived from `[b"pool3"]`. */
export const POOL_PDA: Address = address("B89Yhoecj9AKJEDXT49DfjbTJoqjovmcKgYmqdzQwBYf");

/** Merkle tree depth — must match `spec.json` and on-chain constant. */
export const TREE_DEPTH = 20;

/**
 * Number of splits per deposit — must match `spec.json` (`circuit.split_count`)
 * and on-chain `constants::SPLIT_COUNT`.
 */
export const SPLIT_COUNT = 3;

/** Minimum deposit in lamports — must match `constants::MIN_DEPOSIT_AMOUNT`. */
export const MIN_DEPOSIT_AMOUNT = 1_000_000n;

/** Lamports per SOL. */
export const LAMPORTS_PER_SOL = 1_000_000_000n;
