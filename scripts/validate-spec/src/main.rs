//! Validates that `circuits/withdrawal/spec.json` is internally consistent
//! and matches its consumers (.nr, Rust, TypeScript).
//!
//! Stage 2.1.2 — modular structure.

mod project;
mod rules;
mod spec;

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let project_root = project::find_project_root()?;
    println!("📁 Project root: {}", project_root.display());

    let spec_path = project_root.join("circuits/withdrawal/spec.json");
    println!("📄 Spec: {}", spec_path.display());

    let raw = std::fs::read_to_string(&spec_path)
        .with_context(|| format!("Failed to read {}", spec_path.display()))?;

    let spec: spec::Spec =
        serde_json::from_str(&raw).context("Failed to parse spec.json as JSON")?;

    spec::print_summary(&spec)?;

    rules::validate_all(&spec)?;

    println!();
    println!("✅ Validation passed.");
    Ok(())
}
