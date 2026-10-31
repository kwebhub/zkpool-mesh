//! Tests for the Fastify HTTP server (via app.inject — no port binding).

import { test } from "node:test";
import assert from "node:assert/strict";
import { buildApp } from "../src/app.js";

const ONE = "0".repeat(63) + "1";
const TWO = "0".repeat(63) + "2";
const COMMITMENT = "09d9d188784ab20199a5eb7267ce27765a374ce6bc672deee9eeac9ba90b80fc";

async function makeApp() {
  return buildApp({ logger: false });
}

test("GET /health returns ok", async () => {
  const app = await makeApp();
  const r = await app.inject({ method: "GET", url: "/health" });
  assert.equal(r.statusCode, 200);
  assert.deepEqual(r.json(), { status: "ok" });
  await app.close();
});

test("POST /hash returns bare hex matching known value", async () => {
  const app = await makeApp();
  const r = await app.inject({
    method: "POST",
    url: "/hash",
    payload: { left: ONE, right: TWO },
  });
  assert.equal(r.statusCode, 200);
  assert.deepEqual(r.json(), {
    hash: "299bfccd7daf3c917e51291383929049ec0eaed800af245056cbf135f7dea636",
  });
  await app.close();
});

test("POST /hash rejects short hex", async () => {
  const app = await makeApp();
  const r = await app.inject({
    method: "POST",
    url: "/hash",
    payload: { left: "00", right: TWO },
  });
  assert.equal(r.statusCode, 400);
  await app.close();
});

test("POST /hash rejects 0x-prefixed hex", async () => {
  const app = await makeApp();
  const r = await app.inject({
    method: "POST",
    url: "/hash",
    payload: { left: "0x" + ONE, right: TWO },
  });
  assert.equal(r.statusCode, 400);
  await app.close();
});

test("POST /root returns root for single commitment", async () => {
  const app = await makeApp();
  const r = await app.inject({
    method: "POST",
    url: "/root",
    payload: { commitments: [COMMITMENT] },
  });
  assert.equal(r.statusCode, 200);
  const body = r.json();
  assert.match(body.root, /^[0-9a-f]{64}$/);
  await app.close();
});

test("POST /root rejects non-array commitments", async () => {
  const app = await makeApp();
  const r = await app.inject({
    method: "POST",
    url: "/root",
    payload: { commitments: "not-array" },
  });
  assert.equal(r.statusCode, 400);
  await app.close();
});

test("POST /proof returns 20-element proof + is_even", async () => {
  const app = await makeApp();
  const r = await app.inject({
    method: "POST",
    url: "/proof",
    payload: { commitments: [COMMITMENT], leaf_index: 0 },
  });
  assert.equal(r.statusCode, 200);
  const body = r.json();
  assert.equal(body.proof.length, 20);
  assert.equal(body.is_even.length, 20);
  await app.close();
});

test("POST /proof rejects out-of-range leaf_index", async () => {
  const app = await makeApp();
  const r = await app.inject({
    method: "POST",
    url: "/proof",
    payload: { commitments: [COMMITMENT], leaf_index: 5 },
  });
  assert.equal(r.statusCode, 400);
  await app.close();
});

test("POST /proof rejects non-integer leaf_index", async () => {
  const app = await makeApp();
  const r = await app.inject({
    method: "POST",
    url: "/proof",
    payload: { commitments: [COMMITMENT], leaf_index: 1.5 },
  });
  assert.equal(r.statusCode, 400);
  await app.close();
});
