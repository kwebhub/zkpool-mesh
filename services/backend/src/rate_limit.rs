//! Rate limiting middleware for axum.
//!
//! Uses Redis INCR + EXPIRE, keyed by client IP + endpoint. The counter
//! resets when its TTL expires.
//!
//! ## Algorithm
//!
//! For each request:
//!   1. Extract client IP from `X-Forwarded-For` header (or `X-Real-IP`,
//!      or fallback to "unknown").
//!   2. Build key: `rate:{endpoint}:{ip}`.
//!   3. `INCR key` → count. If `count == 1`, set `EXPIRE key ttl`.
//!   4. If `count > limit` → return `429 Too Many Requests` with
//!      `Retry-After` header.
//!   5. Otherwise → proceed.
//!
//! ## Two tiers
//!
//! - **withdraw** — expensive, hits the prover. Default 5/min.
//! - **read** — cheap, but still needs protection from scraping.
//!   Default 60/min.
//!
//! Endpoints `/api/health` and `/metrics` are **not** rate-limited.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::cache::Cache;

/// Rate limit tier.
#[derive(Debug, Clone, Copy)]
pub enum Tier {
    /// Withdraw — 5/min by default.
    Withdraw,
    /// Read — 60/min by default.
    Read,
}

impl Tier {
    fn endpoint_label(&self) -> &'static str {
        match self {
            Tier::Withdraw => "withdraw",
            Tier::Read => "read",
        }
    }
}

/// Rate limiter state shared via axum `State`.
#[derive(Clone)]
pub struct RateLimiter {
    cache: Arc<tokio::sync::Mutex<Cache>>,
    withdraw_limit: u64,
    read_limit: u64,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new(cache: Cache, withdraw_limit: u64, read_limit: u64) -> Self {
        Self {
            cache: Arc::new(tokio::sync::Mutex::new(cache)),
            withdraw_limit,
            read_limit,
        }
    }

    /// Get the limit for a tier.
    fn limit(&self, tier: Tier) -> u64 {
        match tier {
            Tier::Withdraw => self.withdraw_limit,
            Tier::Read => self.read_limit,
        }
    }

    /// Check a single request. Returns `Ok(remaining)` or an error string
    /// (with the number of seconds to wait) when the limit is exceeded.
    pub async fn check(&self, tier: Tier, ip: &str) -> Result<u64, u64> {
        let limit = self.limit(tier);
        let ttl = 60u64; // 1 minute window
        let key = format!("rate:{}:{}", tier.endpoint_label(), ip);

        let mut cache = self.cache.lock().await;
        let count = match cache.incr_with_ttl(&key, ttl).await {
            Ok(c) => c,
            Err(_) => {
                // On Redis failure, fail-open: allow the request.
                return Ok(limit);
            }
        };

        if count > limit {
            Err(ttl)
        } else {
            Ok(limit - count)
        }
    }
}

/// Extracts the client IP from a request.
///
/// Order of preference:
///   1. `X-Forwarded-For` (first value).
///   2. `X-Real-IP`.
///   3. `ConnectInfo<SocketAddr>` (from axum).
///   4. "unknown".
fn client_ip(req: &Request) -> String {
    if let Some(fwd) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = fwd.to_str() {
            if let Some(first) = s.split(',').next() {
                return first.trim().to_string();
            }
        }
    }

    if let Some(real) = req.headers().get("x-real-ip") {
        if let Ok(s) = real.to_str() {
            return s.trim().to_string();
        }
    }

    if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
        return addr.ip().to_string();
    }

    "unknown".to_string()
}

/// Middleware for the `Withdraw` tier. Applied to POST /api/withdraw.
pub async fn withdraw_middleware(
    State(limiter): State<RateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&req);
    match limiter.check(Tier::Withdraw, &ip).await {
        Ok(_remaining) => next.run(req).await,
        Err(retry_after) => rate_limit_response(retry_after),
    }
}

/// Middleware for the `Read` tier. Applied to GET /api/commitments,
/// /api/root, /api/proof.
pub async fn read_middleware(
    State(limiter): State<RateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    let ip = client_ip(&req);
    match limiter.check(Tier::Read, &ip).await {
        Ok(_remaining) => next.run(req).await,
        Err(retry_after) => rate_limit_response(retry_after),
    }
}

/// Standard 429 response with Retry-After header.
fn rate_limit_response(retry_after: u64) -> Response {
    let body = format!(
        "{{\"error\":\"rate limit exceeded\",\"retry_after\":{}}}",
        retry_after
    );
    (
        StatusCode::TOO_MANY_REQUESTS,
        [("Retry-After", retry_after.to_string())],
        body,
    )
        .into_response()
}
