//! Fastify app builder for the Merkle service.
//!
//! This module exports `buildApp()` which constructs and returns a Fastify
//! instance WITHOUT calling `listen()`. That makes it testable via
//! `app.inject()` — no need to bind a port in tests.
//!
//! `src/server.js` imports `buildApp()` and calls `listen()`.
//!
//! Conventions (match services/backend/src/tree.rs):
//!   All hex inputs/outputs are BARE (no 0x prefix), 64 chars = 32 bytes.
//!
//! Endpoints:
//!   GET  /health -> {status: "ok"}
//!   POST /hash   body {left, right}                    -> {hash}
//!   POST /root   body {commitments: [hex]}             -> {root}
//!   POST /proof  body {commitments: [hex], leaf_index} -> {proof, is_even}

import Fastify from "fastify";
import cors from "@fastify/cors";
import { poseidon2Hash } from "./poseidon.js";
import { computeRoot, computeProof, TREE_DEPTH } from "./merkle.js";
import { computeHashes } from "./hashes.js";

const HEX64_RE = /^[0-9a-f]{64}$/;

function isHex64(s) {
  return typeof s === "string" && HEX64_RE.test(s);
}

export async function buildApp({ logger = true } = {}) {
  const app = Fastify({ logger });

  await app.register(cors, { origin: true });

  // --- GET /health ---
  app.get("/health", async () => ({ status: "ok" }));

  // --- POST /hash ---
  app.post("/hash", async (req, reply) => {
    const { left, right } = req.body ?? {};
    if (!isHex64(left) || !isHex64(right)) {
      return reply.code(400).send({
        error: "left and right must be 64-char bare hex strings (no 0x)",
      });
    }
    const hash = await poseidon2Hash(left, right);
    return { hash };
  });

  // --- POST /hashes ---
  app.post("/hashes", async (req, reply) => {
    const { nullifier, secret, amount } = req.body ?? {};
    if (!isHex64(nullifier) || !isHex64(secret) || !isHex64(amount)) {
      return reply.code(400).send({
        error: "nullifier, secret and amount must be 64-char bare hex strings (no 0x)",
      });
    }
    const result = await computeHashes(nullifier, secret, amount);
    return result;
  });

  // --- POST /root ---
  app.post("/root", async (req, reply) => {
    const { commitments } = req.body ?? {};
    if (!Array.isArray(commitments) || !commitments.every(isHex64)) {
      return reply.code(400).send({
        error: "commitments must be an array of 64-char bare hex strings (no 0x)",
      });
    }
    if (commitments.length > 2 ** TREE_DEPTH) {
      return reply.code(400).send({
        error: `too many commitments: ${commitments.length} > 2^${TREE_DEPTH}`,
      });
    }
    const root = await computeRoot(commitments);
    return { root };
  });

  // --- POST /proof ---
  app.post("/proof", async (req, reply) => {
    const { commitments, leaf_index } = req.body ?? {};
    if (!Array.isArray(commitments) || !commitments.every(isHex64)) {
      return reply.code(400).send({
        error: "commitments must be an array of 64-char bare hex strings (no 0x)",
      });
    }
    if (
      typeof leaf_index !== "number" ||
      !Number.isInteger(leaf_index) ||
      leaf_index < 0 ||
      leaf_index >= commitments.length
    ) {
      return reply.code(400).send({
        error: `leaf_index must be an integer in [0, ${commitments.length})`,
      });
    }
    const { proof, isEven } = await computeProof(commitments, leaf_index);
    return { proof, is_even: isEven };
  });

  return app;
}
