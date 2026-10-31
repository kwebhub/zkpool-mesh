//! Locating the project root.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Finds the project root (directory containing `circuits/` and `onchain/`).
pub fn find_project_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("Failed to get current dir")?;

    loop {
        if dir.join("circuits").is_dir() && dir.join("onchain").is_dir() {
            return Ok(dir);
        }
        if !dir.pop() {
            anyhow::bail!(
                "Could not find project root (directory with `circuits/` and `onchain/`)"
            );
        }
    }
}
