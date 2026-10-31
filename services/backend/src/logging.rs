//! Logging initialization.
//!
//! Uses `tracing` + `tracing-subscriber`. Two modes:
//!   - `RUST_LOG` unset or `LOG_FORMAT=pretty` (default in dev):
//!     human-readable output with colors.
//!   - `LOG_FORMAT=json` (default in production):
//!     JSON lines, one per log record, for log collectors (Loki, Elastic).
//!
//! Call `init_logging()` once at startup, before any `tracing::info!` etc.

use anyhow::Result;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize the global tracing subscriber.
///
/// Reads `RUST_LOG` for the filter (default `info`) and `LOG_FORMAT` for
/// the output format (`pretty` or `json`, default `pretty`).
pub fn init_logging() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "pretty".into());

    let registry = tracing_subscriber::registry().with(filter);

    match format.as_str() {
        "json" => {
            registry
                .with(fmt::layer().json().with_current_span(true))
                .try_init()
                .map_err(|e| anyhow::anyhow!("failed to init json logging: {}", e))?;
        }
        _ => {
            registry
                .with(fmt::layer().pretty())
                .try_init()
                .map_err(|e| anyhow::anyhow!("failed to init pretty logging: {}", e))?;
        }
    }

    Ok(())
}
