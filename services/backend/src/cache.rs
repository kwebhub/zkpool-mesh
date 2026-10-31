//! Redis cache wrapper.
//!
//! Uses `redis` with `tokio-comp` and `connection-manager` features for
//! automatic reconnection. All operations are async.
//!
//! The cache serves three purposes:
//!   1. Merkle tree state (`tree:*` keys) — see `tree.rs`.
//!   2. Rate limiting counters (`rate:*` keys) — see `rate_limit.rs`.
//!   3. Response caching for `/api/commitments` and `/api/root`.

use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

/// Cache wrapper around a Redis connection.
///
/// The connection is a `ConnectionManager`, which automatically reconnects
/// on network failures and multiplexes multiple async commands over one
/// underlying connection.
#[derive(Clone)]
pub struct Cache {
    conn: ConnectionManager,
}

impl Cache {
    /// Connect to Redis at the given URL (e.g. `redis://localhost:6379`).
    pub async fn connect(url: &str) -> Result<Self> {
        let client =
            redis::Client::open(url).with_context(|| format!("invalid Redis URL: {}", url))?;
        let conn = ConnectionManager::new(client)
            .await
            .with_context(|| format!("failed to connect to Redis at {}", url))?;
        Ok(Self { conn })
    }

    /// Get a raw string value.
    pub async fn get(&mut self, key: &str) -> Result<Option<String>> {
        let value: Option<String> = self
            .conn
            .get(key)
            .await
            .with_context(|| format!("Redis GET failed for key {:?}", key))?;
        Ok(value)
    }

    /// Set a raw string value (no TTL).
    pub async fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let _: () = self
            .conn
            .set(key, value)
            .await
            .with_context(|| format!("Redis SET failed for key {:?}", key))?;
        Ok(())
    }

    /// Set a raw string value with TTL (seconds).
    pub async fn set_ex(&mut self, key: &str, value: &str, ttl_secs: u64) -> Result<()> {
        let _: () = self
            .conn
            .set_ex(key, value, ttl_secs)
            .await
            .with_context(|| format!("Redis SETEX failed for key {:?}", key))?;
        Ok(())
    }

    /// Delete a key. Returns `true` if the key existed.
    pub async fn del(&mut self, key: &str) -> Result<bool> {
        let removed: i64 = self
            .conn
            .del(key)
            .await
            .with_context(|| format!("Redis DEL failed for key {:?}", key))?;
        Ok(removed > 0)
    }

    /// Get all keys matching a glob pattern (use sparingly).
    ///
    /// Redis `KEYS` is O(N) over the entire keyspace; use it only for
    /// administrative operations, not on the hot path.
    pub async fn keys(&mut self, pattern: &str) -> Result<Vec<String>> {
        let keys: Vec<String> = self
            .conn
            .keys(pattern)
            .await
            .with_context(|| format!("Redis KEYS failed for pattern {:?}", pattern))?;
        Ok(keys)
    }

    /// Delete all keys matching a glob pattern.
    ///
    /// Used by `invalidate_pool` to drop the whole cached tree state of a
    /// pool when a new deposit arrives.
    pub async fn del_pattern(&mut self, pattern: &str) -> Result<u64> {
        let keys = self.keys(pattern).await?;
        if keys.is_empty() {
            return Ok(0);
        }
        let removed: i64 = self
            .conn
            .del(keys)
            .await
            .with_context(|| format!("Redis DEL (multi) failed for pattern {:?}", pattern))?;
        Ok(removed as u64)
    }

    /// Increment a counter, set TTL on first increment. Returns the new value.
    ///
    /// Used by rate limiting: the counter resets when its TTL expires.
    pub async fn incr_with_ttl(&mut self, key: &str, ttl_secs: u64) -> Result<u64> {
        let new_value: u64 = self
            .conn
            .incr(key, 1u64)
            .await
            .with_context(|| format!("Redis INCR failed for key {:?}", key))?;

        // If this is the first increment, set the TTL.
        // `EXPIRE` returns 1 if the TTL was set, 0 if the key has no TTL
        // or does not exist. We set TTL only when new_value == 1.
        if new_value == 1 {
            let _: i64 = self
                .conn
                .expire(key, ttl_secs as i64)
                .await
                .with_context(|| format!("Redis EXPIRE failed for key {:?}", key))?;
        }

        Ok(new_value)
    }

    /// Invalidate cached data for a specific pool.
    ///
    /// Drops:
    ///   - `cache:commitments:{pool}`
    ///   - `cache:root:{pool}`
    ///   - any `tree:{pool}:*` keys (see `tree.rs`).
    pub async fn invalidate_pool(&mut self, pool: &str) -> Result<()> {
        self.del(&format!("cache:commitments:{}", pool)).await?;
        self.del(&format!("cache:root:{}", pool)).await?;
        self.del_pattern(&format!("tree:{}:*", pool)).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests need a running Redis. Skip them if `REDIS_URL` is not set.
    // They are integration-style tests, executed via `cargo test -- --ignored`.

    #[tokio::test]
    #[ignore]
    async fn test_set_get_del() {
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());
        let mut cache = Cache::connect(&url).await.expect("connect");

        let key = "test:cache:set_get_del";
        cache.set(key, "hello").await.expect("set");
        assert_eq!(
            cache.get(key).await.expect("get"),
            Some("hello".to_string())
        );

        let removed = cache.del(key).await.expect("del");
        assert!(removed);
        assert_eq!(cache.get(key).await.expect("get"), None);
    }

    #[tokio::test]
    #[ignore]
    async fn test_incr_with_ttl() {
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());
        let mut cache = Cache::connect(&url).await.expect("connect");

        let key = "test:cache:incr_with_ttl";
        cache.del(key).await.ok(); // reset

        let v1 = cache.incr_with_ttl(key, 60).await.expect("incr");
        assert_eq!(v1, 1);
        let v2 = cache.incr_with_ttl(key, 60).await.expect("incr");
        assert_eq!(v2, 2);
        let v3 = cache.incr_with_ttl(key, 60).await.expect("incr");
        assert_eq!(v3, 3);

        cache.del(key).await.ok();
    }
}
