//! zkpool-prover binary entry point.

use std::sync::Arc;

use anyhow::Result;
use zkpool_prover::config::Config;
use zkpool_prover::prover::Prover;
use zkpool_prover::server::{build_router, AppState};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env()?;
    let port = config.port;

    let prover = Prover::new(config);
    let state = Arc::new(AppState { prover });
    let app = build_router(state);

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("prover listening on {}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
