-- Initial schema for zkpool_backend.
--
-- Three tables:
--   commitments  — every commitment ever inserted into the Merkle tree.
--   roots        — history of Merkle roots (mirror of PoolState.roots).
--   nullifiers   — every spent nullifier (prevents double-spend at the DB level).
--
-- NOTE: `commitments` intentionally does NOT have a UNIQUE constraint on
-- `(pool_address, tx_signature)`. This is to preserve compatibility with the
-- future "split deposit" feature, where one transaction may produce several
-- commitments. See PROJECT_CONTEXT.md, section 7.6.

CREATE TABLE IF NOT EXISTS commitments (
    id           BIGSERIAL PRIMARY KEY,
    leaf_index   BIGINT NOT NULL UNIQUE,
    commitment   BYTEA NOT NULL CHECK (octet_length(commitment) = 32),
    pool_address TEXT NOT NULL,
    tx_signature TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS roots (
    id           BIGSERIAL PRIMARY KEY,
    root         BYTEA NOT NULL CHECK (octet_length(root) = 32),
    leaf_index   BIGINT NOT NULL,
    pool_address TEXT NOT NULL,
    tx_signature TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS nullifiers (
    nullifier_hash BYTEA PRIMARY KEY CHECK (octet_length(nullifier_hash) = 32),
    pool_address   TEXT NOT NULL,
    recipient      TEXT,
    amount         BIGINT,
    tx_signature   TEXT,
    used_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common queries.

CREATE INDEX IF NOT EXISTS commitments_pool_leaf_idx
    ON commitments (pool_address, leaf_index);

CREATE INDEX IF NOT EXISTS roots_pool_id_idx
    ON roots (pool_address, id DESC);

CREATE INDEX IF NOT EXISTS nullifiers_pool_idx
    ON nullifiers (pool_address);
