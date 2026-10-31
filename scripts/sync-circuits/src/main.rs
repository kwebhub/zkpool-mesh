//! CLI utility to compile Noir circuits and sync JSON artifacts to consumers.
//!
//! ## Modes
//!
//! - `check` (default, for CI): compile, hash, compare. On mismatch —
//!   error, exit non-zero, do NOT copy.
//! - `apply` (for local use): on mismatch — copy, print warning, exit 0.
//!
//! ## Why two modes
//!
//! Silent overwriting on mismatch can hide a real problem (e.g. someone
//! reverted `circuits/` but consumers remained newer). In CI we want to
//! **detect** drift, not mask it. Locally, after editing a circuit, we
//! want to **apply** the update.
//!
//! ## Usage
//!
//! Inside the container:
//!   cd /home/ubuntu
//!   cargo run --manifest-path scripts/sync-circuits/Cargo.toml --release -- check
//!   cargo run --manifest-path scripts/sync-circuits/Cargo.toml --release -- apply

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Circuits to compile and sync.
/// (name, source_dir, target_json_filename)
const CIRCUITS: &[(&str, &str, &str)] = &[
    ("hash2", "circuits/hash2", "hash2.json"),
    ("hashes", "circuits/hashes", "hashes.json"),
    ("withdrawal", "circuits/withdrawal", "withdrawal.json"),
];

/// Destinations to copy the JSON files to.
const DESTINATIONS: &[&str] = &["services/merkle/circuits", "web/public/circuits"];

/// Mode of operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// CI mode: mismatch is an error, do not copy.
    Check,
    /// Local mode: mismatch is a warning, copy.
    Apply,
}

fn main() -> Result<()> {
    let mode = parse_args()?;
    println!("🔧 Mode: {:?}", mode);

    let project_root = find_project_root()?;
    println!("📁 Project root: {}", project_root.display());

    // 1. Compile all circuits.
    for (name, src_dir, _) in CIRCUITS {
        compile_circuit(name, &project_root.join(src_dir))?;
    }

    // 2. Process each circuit.
    let mut total_mismatches = 0usize;
    let mut total_copied = 0usize;

    for (name, src_dir, target_json) in CIRCUITS {
        let source = project_root.join(src_dir).join("target").join(target_json);
        if !source.exists() {
            bail!(
                "Source JSON not found: {}. Did `nargo compile` run?",
                source.display()
            );
        }
        let source_hash = hash_file(&source)?;
        println!();
        println!("📄 {} ({})", target_json, name);
        println!("   source: {}", source.display());
        println!("   sha256: {}", source_hash);

        for dest_dir_rel in DESTINATIONS {
            let dest_dir = project_root.join(dest_dir_rel);
            fs::create_dir_all(&dest_dir)
                .with_context(|| format!("Failed to create {}", dest_dir.display()))?;
            let dest = dest_dir.join(target_json);

            if !dest.exists() {
                // Missing consumer — always safe to copy.
                fs::copy(&source, &dest).with_context(|| {
                    format!("Failed to copy {} → {}", source.display(), dest.display())
                })?;
                println!("   ✓ {} (created)", dest.display());
                total_copied += 1;
                continue;
            }

            let dest_hash = hash_file(&dest)?;
            if source_hash == dest_hash {
                println!("   = {} (in sync)", dest.display());
            } else {
                total_mismatches += 1;
                match mode {
                    Mode::Check => {
                        println!("   ✗ {} (MISMATCH)", dest.display());
                        println!("      source sha256: {}", source_hash);
                        println!("      dest   sha256: {}", dest_hash);
                    }
                    Mode::Apply => {
                        fs::copy(&source, &dest).with_context(|| {
                            format!("Failed to copy {} → {}", source.display(), dest.display())
                        })?;
                        println!("   ⚠ {} (MISMATCH — overwritten)", dest.display());
                        println!("      old sha256: {}", dest_hash);
                        println!("      new sha256: {}", source_hash);
                        total_copied += 1;
                    }
                }
            }
        }
    }

    println!();
    println!("=== Summary ===");
    println!("  copied:     {}", total_copied);
    println!("  mismatches: {}", total_mismatches);

    if mode == Mode::Check && total_mismatches > 0 {
        bail!(
            "check mode failed: {} mismatch(es) between circuits/*/target and consumers",
            total_mismatches
        );
    }

    println!("✅ Done.");
    Ok(())
}

/// Parses CLI arguments. Default mode = `Check`.
fn parse_args() -> Result<Mode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return Ok(Mode::Check);
    }
    if args.len() > 1 {
        bail!("expected at most one argument: check | apply");
    }
    match args[0].as_str() {
        "check" => Ok(Mode::Check),
        "apply" => Ok(Mode::Apply),
        other => bail!("unknown mode {:?} (expected: check | apply)", other),
    }
}

/// Finds the project root (directory containing `circuits/` and `onchain/`).
fn find_project_root() -> Result<PathBuf> {
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

/// Runs `nargo compile` in the given circuit directory.
fn compile_circuit(name: &str, circuit_dir: &Path) -> Result<()> {
    println!();
    println!("🔨 Compiling circuit: {}", name);
    println!("   dir: {}", circuit_dir.display());

    let status = Command::new("nargo")
        .arg("compile")
        .current_dir(circuit_dir)
        .status()
        .with_context(|| format!("Failed to run `nargo compile` in {}", circuit_dir.display()))?;

    if !status.success() {
        bail!(
            "`nargo compile` failed for {} (exit code: {:?})",
            name,
            status.code()
        );
    }

    Ok(())
}

/// Computes the SHA-256 of a file and returns it as lowercase hex.
fn hash_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    Ok(hex::encode(digest))
}
