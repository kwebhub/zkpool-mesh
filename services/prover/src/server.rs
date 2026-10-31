//! axum HTTP server for the prover service.
//!
//! ## Endpoints
//!
//! - `GET  /health` → `{status: "ok"}`
//! - `POST /prove`  → body `WitnessInputs`, response `{proof: hex, public_witness: hex}`
//!
//! ## Hex encoding
//!
//! Request: hex fields may be bare or `0x`-prefixed (witness.rs normalizes).
//! Response: `proof` and `public_witness` are bare hex (matches backend
//! convention — `tree.rs` and the eventual `/api/withdraw` pipeline).

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::prover::{ProofResult, Prover};
use crate::witness::WitnessInputs;

/// Shared state for handlers.
pub struct AppState {
    pub prover: Prover,
}

/// Build the axum router.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/prove", post(prove))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[derive(Serialize)]
struct ProveResponse {
    proof: String,
    public_witness: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

async fn prove(State(state): State<Arc<AppState>>, Json(inputs): Json<WitnessInputs>) -> Response {
    match state.prover.prove(&inputs).await {
        Ok(ProofResult {
            proof,
            public_witness,
        }) => Json(ProveResponse {
            proof: hex::encode(proof),
            public_witness: hex::encode(public_witness),
        })
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("{:#}", e),
            }),
        )
            .into_response(),
    }
}
