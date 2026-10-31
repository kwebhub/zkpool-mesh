//! Postgres database wrapper.
//!
//! Uses `sqlx` with the `postgres` feature. Provides:
//!   - `Db::new(pool)` — wrap an existing `PgPool`.
//!   - `Db::connect(url)` — connect and return a `Db`.
//!   - Insertion methods for commitments, roots, nullifiers.
//!   - Query methods for the API handlers.
//!
//! All inserts use `ON CONFLICT DO NOTHING` for idempotency — the indexer
//! may re-process the same transaction in edge cases (RPC rewinds, etc.).

use anyhow::{Context, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;

/// Database wrapper.
#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

/// A commitment record from the `commitments` table.
#[derive(Debug, Clone)]
pub struct Commitment {
    pub id: i64,
    pub leaf_index: i64,
    pub commitment: Vec<u8>,
    pub pool_address: String,
    pub tx_signature: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Db {
    /// Connect to Postgres and return a `Db`.
    pub async fn connect(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await
            .with_context(|| format!("failed to connect to Postgres at {}", url))?;
        Ok(Self { pool })
    }

    /// Wrap an existing pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a reference to the inner pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Save a commitment to the DB.
    ///
    /// Idempotent: if `leaf_index` already exists, does nothing.
    pub async fn save_commitment(
        &self,
        leaf_index: i64,
        commitment: &[u8],
        pool_address: &str,
        tx_signature: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO commitments (leaf_index, commitment, pool_address, tx_signature)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (leaf_index) DO NOTHING
            "#,
        )
        .bind(leaf_index)
        .bind(commitment)
        .bind(pool_address)
        .bind(tx_signature)
        .execute(&self.pool)
        .await
        .context("failed to insert commitment")?;
        Ok(())
    }

    /// Save a root to the DB.
    pub async fn save_root(
        &self,
        root: &[u8],
        leaf_index: i64,
        pool_address: &str,
        tx_signature: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO roots (root, leaf_index, pool_address, tx_signature)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(root)
        .bind(leaf_index)
        .bind(pool_address)
        .bind(tx_signature)
        .execute(&self.pool)
        .await
        .context("failed to insert root")?;
        Ok(())
    }

    /// Save a nullifier to the DB.
    ///
    /// Idempotent: `nullifier_hash` is the primary key.
    pub async fn save_nullifier(
        &self,
        nullifier_hash: &[u8],
        pool_address: &str,
        recipient: Option<&str>,
        amount: Option<i64>,
        tx_signature: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO nullifiers (nullifier_hash, pool_address, recipient, amount, tx_signature)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (nullifier_hash) DO NOTHING
            "#,
        )
        .bind(nullifier_hash)
        .bind(pool_address)
        .bind(recipient)
        .bind(amount)
        .bind(tx_signature)
        .execute(&self.pool)
        .await
        .context("failed to insert nullifier")?;
        Ok(())
    }

    /// Check if a commitment exists.
    pub async fn commitment_exists(&self, commitment: &[u8]) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM commitments WHERE commitment = $1 LIMIT 1")
            .bind(commitment)
            .fetch_optional(&self.pool)
            .await
            .context("failed to check commitment existence")?;
        Ok(row.is_some())
    }

    /// Check if a nullifier is already used.
    pub async fn is_nullifier_used(&self, nullifier_hash: &[u8]) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM nullifiers WHERE nullifier_hash = $1 LIMIT 1")
            .bind(nullifier_hash)
            .fetch_optional(&self.pool)
            .await
            .context("failed to check nullifier usage")?;
        Ok(row.is_some())
    }

    /// List all commitments for a pool, ordered by leaf_index.
    pub async fn list_commitments(&self, pool_address: &str) -> Result<Vec<Commitment>> {
        let rows = sqlx::query(
            r#"
            SELECT id, leaf_index, commitment, pool_address, tx_signature, created_at
            FROM commitments
            WHERE pool_address = $1
            ORDER BY leaf_index ASC
            "#,
        )
        .bind(pool_address)
        .fetch_all(&self.pool)
        .await
        .context("failed to list commitments")?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            out.push(Commitment {
                id: row.get("id"),
                leaf_index: row.get("leaf_index"),
                commitment: row.get("commitment"),
                pool_address: row.get("pool_address"),
                tx_signature: row.get("tx_signature"),
                created_at: row.get("created_at"),
            });
        }
        Ok(out)
    }

    /// Get the latest root for a pool.
    pub async fn latest_root(&self, pool_address: &str) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query(
            r#"
            SELECT root FROM roots
            WHERE pool_address = $1
            ORDER BY id DESC
            LIMIT 1
            "#,
        )
        .bind(pool_address)
        .fetch_optional(&self.pool)
        .await
        .context("failed to fetch latest root")?;
        Ok(row.map(|r| r.get::<Vec<u8>, _>("root")))
    }

    /// Get the next leaf index (max + 1) for a pool.
    pub async fn next_leaf_index(&self, pool_address: &str) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COALESCE(MAX(leaf_index), -1) + 1 AS next_index
            FROM commitments
            WHERE pool_address = $1
            "#,
        )
        .bind(pool_address)
        .fetch_one(&self.pool)
        .await
        .context("failed to compute next leaf index")?;
        let next: i64 = row.get("next_index");
        Ok(next)
    }
}
