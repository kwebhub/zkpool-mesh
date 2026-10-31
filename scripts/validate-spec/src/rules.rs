//! Validation rules for `circuits/withdrawal/spec.json`.
//!
//! Stage 2.1.2 — spec-internal consistency.
//! Stage 15.2 — updated for split deposit: 6 public inputs, 5 constraints, SPLIT_COUNT.

use crate::spec::Spec;
use anyhow::{bail, Result};

/// Expected number of public inputs (Stage 15: was 5, now 6 with total_amount).
const EXPECTED_NR_PUBLIC_INPUTS: usize = 6;

/// Expected total witness bytes: 12-byte header + 6 × 32-byte public inputs.
const EXPECTED_WITNESS_BYTES: u64 = 204;

/// Expected header size in bytes.
const EXPECTED_HEADER_BYTES: u64 = 12;

/// Expected public section size in bytes (Stage 15: 6 × 32 = 192).
const EXPECTED_PUBLIC_SECTION_BYTES: u64 = 192;

/// Expected sum of raw data bytes: 32+32+32+32+8+8 = 144.
const EXPECTED_RAW_BYTES: u64 = 144;

/// Expected tree depth.
const EXPECTED_TREE_DEPTH: u64 = 20;

/// Expected split count (Stage 15).
const EXPECTED_SPLIT_COUNT: u64 = 3;

/// Expected constraint ids in order (Stage 15: added C4, C5).
const EXPECTED_CONSTRAINT_IDS: [&str; 5] = ["C1", "C2", "C3", "C4", "C5"];

/// Expected length of `merkle_proof` and `is_even` private inputs.
const EXPECTED_MERKLE_PROOF_LENGTH: u64 = 20;

/// Runs all spec-internal validation rules.
pub fn validate_all(spec: &Spec) -> Result<()> {
    println!();
    println!("=== Running validation rules ===");

    rule_version(spec)?;
    rule_circuit_name(spec)?;
    rule_tree_depth(spec)?;
    rule_split_count(spec)?;
    rule_nr_public_inputs(spec)?;
    rule_public_inputs_unique_names(spec)?;
    rule_public_inputs_bytes_sum(spec)?;
    rule_public_inputs_bytes_match_type(spec)?;
    rule_private_inputs_lengths(spec)?;
    rule_witness_total_bytes(spec)?;
    rule_witness_header_bytes(spec)?;
    rule_witness_public_section_bytes(spec)?;
    rule_constraints(spec)?;
    rule_hash_functions(spec)?;
    rule_artifacts_json(spec)?;
    rule_consumers_count(spec)?;
    rule_checkpoints_non_empty(spec)?;

    println!("✅ All 17 rules passed.");
    Ok(())
}

fn rule_version(spec: &Spec) -> Result<()> {
    if spec.version != 1 {
        bail!("rule_version: expected version 1, got {}", spec.version);
    }
    println!("  ✓ version == 1");
    Ok(())
}

fn rule_circuit_name(spec: &Spec) -> Result<()> {
    if spec.circuit.name != "withdrawal" {
        bail!(
            "rule_circuit_name: expected circuit.name == \"withdrawal\", got {:?}",
            spec.circuit.name
        );
    }
    println!("  ✓ circuit.name == \"withdrawal\"");
    Ok(())
}

fn rule_tree_depth(spec: &Spec) -> Result<()> {
    if spec.circuit.tree_depth != EXPECTED_TREE_DEPTH {
        bail!(
            "rule_tree_depth: expected tree_depth {}, got {}",
            EXPECTED_TREE_DEPTH,
            spec.circuit.tree_depth
        );
    }
    println!("  ✓ circuit.tree_depth == {}", EXPECTED_TREE_DEPTH);
    Ok(())
}

fn rule_split_count(spec: &Spec) -> Result<()> {
    if spec.circuit.split_count != EXPECTED_SPLIT_COUNT {
        bail!(
            "rule_split_count: expected split_count {}, got {}",
            EXPECTED_SPLIT_COUNT,
            spec.circuit.split_count
        );
    }
    // The splits private input length must match split_count.
    let splits = spec
        .private_inputs
        .iter()
        .find(|pi| pi.name == "splits")
        .ok_or_else(|| {
            anyhow::anyhow!("rule_split_count: private_inputs[] has no entry named \"splits\"")
        })?;
    match splits.length {
        Some(len) if len == EXPECTED_SPLIT_COUNT => {}
        other => bail!(
            "rule_split_count: splits.length must equal split_count {} (got {:?})",
            EXPECTED_SPLIT_COUNT,
            other
        ),
    }
    println!(
        "  ✓ circuit.split_count == {} and splits.length == {}",
        EXPECTED_SPLIT_COUNT, EXPECTED_SPLIT_COUNT
    );
    Ok(())
}

fn rule_nr_public_inputs(spec: &Spec) -> Result<()> {
    let declared = spec.circuit.nr_public_inputs as usize;
    let actual = spec.public_inputs.len();
    if declared != actual {
        bail!(
            "rule_nr_public_inputs: circuit.nr_public_inputs == {} but public_inputs.len() == {}",
            declared,
            actual
        );
    }
    if declared != EXPECTED_NR_PUBLIC_INPUTS {
        bail!(
            "rule_nr_public_inputs: expected {} public inputs, got {}",
            EXPECTED_NR_PUBLIC_INPUTS,
            declared
        );
    }
    println!(
        "  ✓ circuit.nr_public_inputs == public_inputs.len() == {}",
        declared
    );
    Ok(())
}

fn rule_public_inputs_unique_names(spec: &Spec) -> Result<()> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    for pi in &spec.public_inputs {
        if !seen.insert(&pi.name) {
            bail!(
                "rule_public_inputs_unique_names: duplicate name {:?}",
                pi.name
            );
        }
    }
    println!("  ✓ public_inputs[].name are unique");
    Ok(())
}

fn rule_public_inputs_bytes_sum(spec: &Spec) -> Result<()> {
    // Sum of witness slots: padded_to if present, otherwise bytes.
    let padded_sum: u64 = spec
        .public_inputs
        .iter()
        .map(|pi| pi.padded_to.unwrap_or(pi.bytes))
        .sum();
    if padded_sum != EXPECTED_PUBLIC_SECTION_BYTES {
        bail!(
            "rule_public_inputs_bytes_sum: expected sum of witness slots {} bytes, got {}",
            EXPECTED_PUBLIC_SECTION_BYTES,
            padded_sum
        );
    }

    // Sum of raw data bytes — sanity check, expected 32+32+32+32+8+8 = 144.
    let raw_sum: u64 = spec.public_inputs.iter().map(|pi| pi.bytes).sum();
    if raw_sum != EXPECTED_RAW_BYTES {
        bail!(
            "rule_public_inputs_bytes_sum: expected sum of raw bytes {}, got {}",
            EXPECTED_RAW_BYTES,
            raw_sum
        );
    }

    println!(
        "  ✓ public_inputs: raw sum == {} bytes, witness slot sum == {} bytes",
        EXPECTED_RAW_BYTES, EXPECTED_PUBLIC_SECTION_BYTES
    );
    Ok(())
}

fn rule_public_inputs_bytes_match_type(spec: &Spec) -> Result<()> {
    for pi in &spec.public_inputs {
        let expected = match pi.ty.as_str() {
            "field" => 32u64,
            "pubkey" => 32u64,
            "u64" => 8u64,
            other => bail!(
                "rule_public_inputs_bytes_match_type: unknown type {:?} for input {:?}",
                other,
                pi.name
            ),
        };
        if pi.bytes != expected {
            bail!(
                "rule_public_inputs_bytes_match_type: input {:?} has type {:?} which expects {} bytes, but declared {} bytes",
                pi.name,
                pi.ty,
                expected,
                pi.bytes
            );
        }
        // For u64, verify padded_to == 32 and padding == "right_aligned".
        if pi.ty == "u64" {
            match (pi.padded_to, pi.padding.as_deref()) {
                (Some(32), Some("right_aligned")) => {}
                other => bail!(
                    "rule_public_inputs_bytes_match_type: input {:?} (u64) must have padded_to=32 and padding=\"right_aligned\", got {:?}",
                    pi.name,
                    other
                ),
            }
        }
    }
    println!("  ✓ public_inputs[].bytes match declared types");
    Ok(())
}

fn rule_private_inputs_lengths(spec: &Spec) -> Result<()> {
    // merkle_proof and is_even must have length == TREE_DEPTH.
    for name in ["merkle_proof", "is_even"] {
        let pi = spec
            .private_inputs
            .iter()
            .find(|pi| pi.name == name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "rule_private_inputs_lengths: private_inputs[] has no entry named {:?}",
                    name
                )
            })?;
        match pi.length {
            Some(len) if len == EXPECTED_MERKLE_PROOF_LENGTH => {}
            other => bail!(
                "rule_private_inputs_lengths: {:?}.length must be {} (got {:?})",
                name,
                EXPECTED_MERKLE_PROOF_LENGTH,
                other
            ),
        }
    }
    // splits.length is validated in rule_split_count.
    // note_index has no length (scalar).
    println!(
        "  ✓ private_inputs: merkle_proof.length == is_even.length == {}",
        EXPECTED_MERKLE_PROOF_LENGTH
    );
    Ok(())
}

fn rule_witness_total_bytes(spec: &Spec) -> Result<()> {
    if spec.witness_layout.total_bytes != EXPECTED_WITNESS_BYTES {
        bail!(
            "rule_witness_total_bytes: expected {}, got {}",
            EXPECTED_WITNESS_BYTES,
            spec.witness_layout.total_bytes
        );
    }
    let computed =
        spec.witness_layout.header.bytes + spec.witness_layout.public_inputs_section.bytes;
    if computed != EXPECTED_WITNESS_BYTES {
        bail!(
            "rule_witness_total_bytes: header.bytes + public_inputs_section.bytes == {} but total_bytes == {}",
            computed,
            EXPECTED_WITNESS_BYTES
        );
    }
    println!(
        "  ✓ witness_layout.total_bytes == {}",
        EXPECTED_WITNESS_BYTES
    );
    Ok(())
}

fn rule_witness_header_bytes(spec: &Spec) -> Result<()> {
    if spec.witness_layout.header.bytes != EXPECTED_HEADER_BYTES {
        bail!(
            "rule_witness_header_bytes: expected {}, got {}",
            EXPECTED_HEADER_BYTES,
            spec.witness_layout.header.bytes
        );
    }
    println!(
        "  ✓ witness_layout.header.bytes == {}",
        EXPECTED_HEADER_BYTES
    );
    Ok(())
}

fn rule_witness_public_section_bytes(spec: &Spec) -> Result<()> {
    if spec.witness_layout.public_inputs_section.bytes != EXPECTED_PUBLIC_SECTION_BYTES {
        bail!(
            "rule_witness_public_section_bytes: expected {}, got {}",
            EXPECTED_PUBLIC_SECTION_BYTES,
            spec.witness_layout.public_inputs_section.bytes
        );
    }
    println!(
        "  ✓ witness_layout.public_inputs_section.bytes == {}",
        EXPECTED_PUBLIC_SECTION_BYTES
    );
    Ok(())
}

fn rule_constraints(spec: &Spec) -> Result<()> {
    if spec.constraints.len() != EXPECTED_CONSTRAINT_IDS.len() {
        bail!(
            "rule_constraints: expected {} constraints, got {}",
            EXPECTED_CONSTRAINT_IDS.len(),
            spec.constraints.len()
        );
    }
    let ids: Vec<&str> = spec.constraints.iter().map(|c| c.id.as_str()).collect();
    if ids != EXPECTED_CONSTRAINT_IDS {
        bail!(
            "rule_constraints: expected ids {:?}, got {:?}",
            EXPECTED_CONSTRAINT_IDS,
            ids
        );
    }
    println!("  ✓ constraints == {:?}", EXPECTED_CONSTRAINT_IDS);
    Ok(())
}

fn rule_hash_functions(spec: &Spec) -> Result<()> {
    if spec.hash_functions.hash_4.status != "forbidden" {
        bail!(
            "rule_hash_functions: hash_4.status must be \"forbidden\", got {:?}",
            spec.hash_functions.hash_4.status
        );
    }
    let check = |name: &str, sig: &str, expected: &str| -> Result<()> {
        if sig != expected {
            bail!(
                "rule_hash_functions: {} signature mismatch: expected {:?}, got {:?}",
                name,
                expected,
                sig
            );
        }
        Ok(())
    };
    check(
        "hash_1",
        &spec.hash_functions.hash_1.signature,
        "fn hash_1(input: Field) -> Field",
    )?;
    check(
        "hash_2",
        &spec.hash_functions.hash_2.signature,
        "fn hash_2(input1: Field, input2: Field) -> Field",
    )?;
    check(
        "hash_3",
        &spec.hash_functions.hash_3.signature,
        "fn hash_3(input1: Field, input2: Field, input3: Field) -> Field",
    )?;
    println!("  ✓ hash_functions: hash_1, hash_2, hash_3 present; hash_4 forbidden");
    Ok(())
}

fn rule_artifacts_json(spec: &Spec) -> Result<()> {
    if spec.artifacts.json != "withdrawal.json" {
        bail!(
            "rule_artifacts_json: expected \"withdrawal.json\", got {:?}",
            spec.artifacts.json
        );
    }
    println!("  ✓ artifacts.json == \"withdrawal.json\"");
    Ok(())
}

fn rule_consumers_count(spec: &Spec) -> Result<()> {
    if spec.consumers_of_public_layout.len() != 4 {
        bail!(
            "rule_consumers_count: expected 4 consumers, got {}",
            spec.consumers_of_public_layout.len()
        );
    }
    println!("  ✓ consumers_of_public_layout has 4 entries");
    Ok(())
}

fn rule_checkpoints_non_empty(spec: &Spec) -> Result<()> {
    if spec.checkpoints.stage_2.is_empty() {
        bail!("rule_checkpoints_non_empty: checkpoints.stage_2 is empty");
    }
    if spec.checkpoints.stage_3.is_empty() {
        bail!("rule_checkpoints_non_empty: checkpoints.stage_3 is empty");
    }
    println!(
        "  ✓ checkpoints: stage_2 ({} entries), stage_3 ({} entries)",
        spec.checkpoints.stage_2.len(),
        spec.checkpoints.stage_3.len()
    );
    Ok(())
}
