//! zkpool_backend — HTTP API + indexer.
//!
//! Stage 5.9.3: full endpoint set.

use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;
use tracing::{error, info};

use zkpool_backend::api_types::{ProverResponse, WithdrawRequest, WithdrawResponse};
use zkpool_backend::cache::Cache;
use zkpool_backend::config::Config;
use zkpool_backend::db::Db;
use zkpool_backend::indexer::Indexer;
use zkpool_backend::logging;
use zkpool_backend::metrics;
use zkpool_backend::rate_limit::{self, RateLimiter};
use zkpool_backend::tree::{MerkleClient, MerkleTree};

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Db,
    pub cache: Arc<Mutex<Cache>>,
    pub tree: Arc<Mutex<MerkleTree>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Arc::new(Config::from_env()?);
    logging::init_logging()?;
    info!("logging initialized");
    metrics::init_metrics()?;
    info!("metrics initialized");

    let db = Db::connect(&config.database_url).await?;
    info!("connected to Postgres");

    let cache = Cache::connect(&config.redis_url).await?;
    info!("connected to Redis");
    let cache = Arc::new(Mutex::new(cache));

    let merkle_client = MerkleClient::new(config.merkle_url.clone());
    let tree = MerkleTree::new(
        config.pool_address.clone(),
        config.merkle_tree_depth,
        (*cache.lock().await).clone(),
        merkle_client,
    )?;
    let tree = Arc::new(Mutex::new(tree));
    info!("Merkle tree handle created");

    let state = AppState {
        config: config.clone(),
        db: db.clone(),
        cache: cache.clone(),
        tree: tree.clone(),
    };

    let rate_limiter = RateLimiter::new(
        (*cache.lock().await).clone(),
        config.rate_limit_withdraw_per_min,
        config.rate_limit_read_per_min,
    );

    // Spawn indexer.
    {
        let indexer_config = config.clone();
        let indexer_db = db.clone();
        let indexer_cache = cache.clone();
        let indexer_tree = tree.clone();
        tokio::spawn(async move {
            let indexer = Indexer::new(indexer_config, indexer_db, indexer_cache, indexer_tree);
            if let Err(e) = indexer.run().await {
                error!("indexer terminated: {:#}", e);
            }
        });
        info!("indexer spawned");
    }

    let read_routes = Router::new()
        .route("/api/commitments", get(get_commitments))
        .route("/api/root", get(get_root))
        .route("/api/proof", get(get_proof))
        .route("/api/root-preview", post(post_root_preview))
        .layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
            rate_limit::read_middleware,
        ));

    let withdraw_routes = Router::new()
        .route("/api/withdraw", post(post_withdraw))
        .layer(middleware::from_fn_with_state(
            rate_limiter.clone(),
            rate_limit::withdraw_middleware,
        ));

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/metrics", get(metrics_endpoint))
        .merge(read_routes)
        .merge(withdraw_routes)
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("server listening on {}", addr);

    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("server error: {}", e))?;

    Ok(())
}

// ============================================================
// Handlers
// ============================================================

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = state
        .db
        .next_leaf_index(&state.config.pool_address)
        .await
        .is_ok();
    let status = if db_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "status": if db_ok { "ok" } else { "degraded" },
            "db": db_ok,
            "version": env!("CARGO_PKG_VERSION"),
        })),
    )
}

#[derive(Debug, Deserialize)]
struct PoolQuery {
    #[serde(default)]
    pool_address: Option<String>,
}

async fn get_commitments(
    State(state): State<AppState>,
    Query(q): Query<PoolQuery>,
) -> impl IntoResponse {
    let pool = q
        .pool_address
        .unwrap_or_else(|| state.config.pool_address.clone());
    let cache_key = format!("cache:commitments:{}", pool);

    {
        let mut cache = state.cache.lock().await;
        if let Ok(Some(cached)) = cache.get(&cache_key).await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&cached) {
                return (StatusCode::OK, Json(parsed)).into_response();
            }
        }
    }

    let rows = match state.db.list_commitments(&pool).await {
        Ok(rows) => rows,
        Err(e) => {
            error!("list_commitments failed: {:#}", e);
            metrics::record_db_error();
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "db query failed"})),
            )
                .into_response();
        }
    };

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "leaf_index": c.leaf_index,
                "commitment": hex::encode(&c.commitment),
                "pool_address": c.pool_address,
                "tx_signature": c.tx_signature,
                "created_at": c.created_at.to_rfc3339(),
            })
        })
        .collect();

    let response = json!({
        "pool_address": pool,
        "count": items.len(),
        "commitments": items,
    });

    {
        let mut cache = state.cache.lock().await;
        if let Ok(s) = serde_json::to_string(&response) {
            let _ = cache.set_ex(&cache_key, &s, 30).await;
        }
    }

    (StatusCode::OK, Json(response)).into_response()
}

async fn get_root(State(state): State<AppState>, Query(q): Query<PoolQuery>) -> impl IntoResponse {
    let pool = q
        .pool_address
        .unwrap_or_else(|| state.config.pool_address.clone());
    let cache_key = format!("cache:root:{}", pool);

    {
        let mut cache = state.cache.lock().await;
        if let Ok(Some(cached)) = cache.get(&cache_key).await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&cached) {
                return (StatusCode::OK, Json(parsed)).into_response();
            }
        }
    }

    let root = match state.db.latest_root(&pool).await {
        Ok(r) => r,
        Err(e) => {
            error!("latest_root failed: {:#}", e);
            metrics::record_db_error();
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "db query failed"})),
            )
                .into_response();
        }
    };

    let response = match root {
        Some(root_bytes) => json!({
            "pool_address": pool,
            "root": hex::encode(&root_bytes),
        }),
        None => json!({
            "pool_address": pool,
            "root": null,
        }),
    };

    {
        let mut cache = state.cache.lock().await;
        if let Ok(s) = serde_json::to_string(&response) {
            let _ = cache.set_ex(&cache_key, &s, 30).await;
        }
    }

    (StatusCode::OK, Json(response)).into_response()
}

#[derive(Debug, Deserialize)]
struct ProofQuery {
    #[serde(default)]
    pool_address: Option<String>,
    /// Leaf index. Upper bound is 2^TREE_DEPTH = 1M. Reject anything larger
    /// before forwarding to the Merkle service.
    leaf_index: u64,
}

/// Max leaves in a pool (2^TREE_DEPTH = 2^20).
const MAX_LEAVES: u64 = 1 << 20;

async fn get_proof(
    State(state): State<AppState>,
    Query(q): Query<ProofQuery>,
) -> impl IntoResponse {
    let pool = q
        .pool_address
        .unwrap_or_else(|| state.config.pool_address.clone());

    // Load commitments in leaf_index order.
    let rows = match state.db.list_commitments(&pool).await {
        Ok(rows) => rows,
        Err(e) => {
            error!("list_commitments failed: {:#}", e);
            metrics::record_db_error();
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "db query failed"})),
            )
                .into_response();
        }
    };

    // Reject out-of-bound before touching the DB or Merkle service.
    if q.leaf_index >= MAX_LEAVES {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!("leaf_index must be < {}", MAX_LEAVES),
                "leaf_index": q.leaf_index,
            })),
        )
            .into_response();
    }

    if q.leaf_index as usize >= rows.len() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "leaf_index out of range",
                "count": rows.len(),
                "leaf_index": q.leaf_index,
            })),
        )
            .into_response();
    }

    // Build commitments list (bare hex) for the Merkle service.
    let commitments: Vec<String> = rows.iter().map(|c| hex::encode(&c.commitment)).collect();

    let url = format!("{}/proof", state.config.merkle_url);
    let body = json!({
        "commitments": commitments,
        "leaf_index": q.leaf_index,
    });

    let resp = reqwest::Client::new().post(&url).json(&body).send().await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            error!("merkle request failed: {:#}", e);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "merkle service unreachable"})),
            )
                .into_response();
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        error!("merkle {}: {}", status, body);
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "merkle service error"})),
        )
            .into_response();
    }

    match resp.json::<serde_json::Value>().await {
        Ok(v) => {
            let proof = v.get("proof").cloned().unwrap_or(json!([]));
            let is_even = v.get("is_even").cloned().unwrap_or(json!([]));
            (
                StatusCode::OK,
                Json(json!({
                    "pool_address": pool,
                    "leaf_index": q.leaf_index,
                    "proof": proof,
                    "is_even": is_even,
                })),
            )
                .into_response()
        }
        Err(e) => {
            error!("merkle response parse error: {:#}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "merkle returned invalid response"})),
            )
                .into_response()
        }
    }
}

/// Request body for `POST /api/root-preview`.
#[derive(Debug, Deserialize)]
struct RootPreviewRequest {
    /// Commitment list (bare hex, 64 chars each), in insertion order.
    commitments: Vec<String>,
}

/// Upper bound for `/api/root-preview` payloads — matches the on-chain
/// `MAX_LEAVES` (2^TREE_DEPTH = 2^20).
const MAX_PREVIEW_COMMITMENTS: usize = 1 << 20;

/// Ensure a string is exactly 64 chars of lowercase hex.
fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// `POST /api/root-preview` — compute the Merkle root for a hypothetical
/// commitment list by proxying to the Merkle service `/root`.
///
/// Used by the frontend before a deposit: the new root must be submitted
/// as an argument to the on-chain `deposit` instruction, and it must match
/// what the backend indexer will compute after the event.
async fn post_root_preview(
    State(state): State<AppState>,
    Json(req): Json<RootPreviewRequest>,
) -> impl IntoResponse {
    // Validate shape and size before forwarding.
    if req.commitments.len() > MAX_PREVIEW_COMMITMENTS {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!(
                    "too many commitments: {} > {}",
                    req.commitments.len(),
                    MAX_PREVIEW_COMMITMENTS
                ),
            })),
        )
            .into_response();
    }
    for (i, c) in req.commitments.iter().enumerate() {
        if !is_hex64(c) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": format!("commitments[{}] must be 64-char hex", i),
                })),
            )
                .into_response();
        }
    }

    let url = format!("{}/root", state.config.merkle_url);
    let body = json!({ "commitments": req.commitments });

    let resp = reqwest::Client::new().post(&url).json(&body).send().await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            error!("merkle request failed: {:#}", e);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "merkle service unreachable"})),
            )
                .into_response();
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let body: String = resp.text().await.unwrap_or_default();
        error!("merkle {}: {}", status, body);
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "merkle service error"})),
        )
            .into_response();
    }

    match resp.json::<serde_json::Value>().await {
        Ok(v) => (
            StatusCode::OK,
            Json(json!({
                "root": v.get("root").cloned().unwrap_or(json!(null)),
            })),
        )
            .into_response(),
        Err(e) => {
            error!("merkle response parse error: {:#}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "merkle returned invalid response"})),
            )
                .into_response()
        }
    }
}

async fn metrics_endpoint() -> impl IntoResponse {
    let body = metrics::render_metrics();
    (
        StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4")],
        body,
    )
}

/// `POST /api/withdraw` — proxy the witness to the prover service.
///
/// Request: `WithdrawRequest` (10 hex fields, bare hex).
/// Response: `WithdrawResponse` (base64-encoded proof + public witness).
///
/// The prover returns bare hex; we convert to base64 here so the wire
/// format for the frontend stays unchanged.
async fn post_withdraw(
    State(state): State<AppState>,
    Json(req): Json<WithdrawRequest>,
) -> impl IntoResponse {
    if let Err(e) = req.validate() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response();
    }

    let url = format!("{}/prove", state.config.prover_url);

    let resp = reqwest::Client::new().post(&url).json(&req).send().await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            error!("prover request failed: {:#}", e);
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "prover unreachable"})),
            )
                .into_response();
        }
    };

    let status = resp.status();
    if status.is_success() {
        let parsed: Result<ProverResponse, _> = resp.json().await;
        match parsed {
            Ok(p) => {
                match hex_to_base64(&p.proof)
                    .and_then(|proof| hex_to_base64(&p.public_witness).map(|pw| (proof, pw)))
                {
                    Ok((proof, public_witness)) => (
                        StatusCode::OK,
                        Json(WithdrawResponse {
                            proof,
                            public_witness,
                        }),
                    )
                        .into_response(),
                    Err(e) => {
                        error!("prover hex decode error: {:#}", e);
                        (
                            StatusCode::BAD_GATEWAY,
                            Json(json!({"error": "prover returned invalid hex"})),
                        )
                            .into_response()
                    }
                }
            }
            Err(e) => {
                error!("prover response parse error: {:#}", e);
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": "prover returned invalid response"})),
                )
                    .into_response()
            }
        }
    } else if status.is_client_error() {
        let body: String = resp.text().await.unwrap_or_default();
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "invalid witness",
                "prover_status": status.as_u16(),
                "prover_body": body,
            })),
        )
            .into_response()
    } else {
        let body: String = resp.text().await.unwrap_or_default();
        error!("prover 5xx: status={}, body={}", status, body);
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "prover internal error"})),
        )
            .into_response()
    }
}

/// Decode a bare-hex string into base64 (standard alphabet, with padding).
fn hex_to_base64(hex_str: &str) -> Result<String> {
    use base64::Engine;
    let bytes = hex::decode(hex_str)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}
