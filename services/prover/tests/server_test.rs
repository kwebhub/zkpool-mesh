//! HTTP-level integration tests for the prover server.
//!
//! Uses `tower::ServiceExt::oneshot` — in-process requests, no port binding.
//!
//! Stage 15.7: `WitnessInputs` now has 13 fields (6 public + 7 private).

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use zkpool_prover::config::Config;
use zkpool_prover::prover::Prover;
use zkpool_prover::server::{build_router, AppState};

fn test_state() -> Arc<AppState> {
    let config = Config {
        port: 4002,
        circuit_dir: "/home/ubuntu/circuits/withdrawal".to_string(),
        nargo_bin: "nargo".to_string(),
        sunspot_bin: "sunspot".to_string(),
        nargo_timeout_secs: 60,
        sunspot_timeout_secs: 60,
    };
    Arc::new(AppState {
        prover: Prover::new(config),
    })
}

fn valid_request_body() -> serde_json::Value {
    let merkle_proof: Vec<String> = std::iter::once("07b5bad595e238e3".to_string())
        .chain(std::iter::repeat("00".to_string()).take(19))
        .collect();
    let is_even: Vec<bool> = vec![true; 20];

    serde_json::json!({
        "root": "0178bf57a93031d2ebc274f5134fff54be52fa57cb5e5f1052ce2fd7bac3bf80",
        "nullifier_hash": "1412cc9d862599e6869a1881c8562062b98537456d9035819288219b5cd3e6e4",
        "recipient": "062afbde1181c71c",
        "recipient_binding": "200cfdb247b0436ba0483327abcf8f83ed65f75a0db8d9ebbb611561d7d3b2e1",
        "amount": "0493e0",
        "total_amount": "0f4240",
        "nullifier": "018abef7846071c7",
        "secret": "03157def08c0e38e",
        "note_secret": "04a03ce68d215555",
        "merkle_proof": merkle_proof,
        "is_even": is_even,
        "splits": ["07a120", "0493e0", "030d40"],
        "note_index": 1
    })
}

#[tokio::test]
async fn test_health() {
    let app = build_router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["status"], "ok");
}

#[tokio::test]
async fn test_prove_bad_payload_returns_422() {
    // Empty JSON object — missing required fields.
    let app = build_router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/prove")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_prove_wrong_merkle_len_returns_500() {
    let app = build_router(test_state());
    let mut body = valid_request_body();
    // Truncate merkle_proof to 19 elements — validate() rejects.
    body["merkle_proof"] = serde_json::json!(vec!["00"; 19]);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/prove")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_prove_wrong_splits_len_returns_500() {
    let app = build_router(test_state());
    let mut body = valid_request_body();
    // Two splits instead of three — validate() rejects.
    body["splits"] = serde_json::json!(["07a120", "0493e0"]);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/prove")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_prove_note_index_out_of_bounds_returns_500() {
    let app = build_router(test_state());
    let mut body = valid_request_body();
    // note_index = 5 is >= SPLIT_COUNT = 3 — validate() rejects.
    body["note_index"] = serde_json::json!(5);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/prove")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
#[ignore = "requires nargo + sunspot + circuit artifacts"]
async fn test_prove_real() {
    let app = build_router(test_state());
    let body = valid_request_body();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/prove")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // Stage 15.4: 388 B proof = 776 hex chars, 204 B pw = 408 hex chars.
    let proof_hex = v["proof"].as_str().unwrap();
    let pw_hex = v["public_witness"].as_str().unwrap();
    assert_eq!(proof_hex.len(), 388 * 2);
    assert_eq!(pw_hex.len(), 204 * 2);
}
