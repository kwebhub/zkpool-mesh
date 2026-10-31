//! zkpool_backend library.
//!
//! Modules:
//!   - `api_types`  — shared types between bin and lib.
//!   - `config`     — environment configuration.
//!   - `db`         — Postgres database wrapper.
//!   - `cache`      — Redis cache wrapper.
//!   - `tree`       — incremental Merkle tree (Redis + Merkle service).
//!   - `logging`    — tracing initialization.
//!   - `metrics`    — custom Prometheus metrics.
//!   - `rate_limit` — Redis-based rate limiting middleware.
//!   - `indexer`    — background worker: Solana RPC → DB + tree.

pub mod api_types;
pub mod cache;
pub mod config;
pub mod db;
pub mod indexer;
pub mod logging;
pub mod metrics;
pub mod rate_limit;
pub mod tree;
