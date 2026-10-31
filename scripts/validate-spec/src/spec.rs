//! Typed representation of `circuits/withdrawal/spec.json`.

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Spec {
    pub version: u64,
    #[serde(default)]
    pub description: String,
    pub circuit: Circuit,
    pub public_inputs: Vec<PublicInput>,
    pub private_inputs: Vec<PrivateInput>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    pub hash_functions: HashFunctions,
    pub witness_layout: WitnessLayout,
    pub artifacts: Artifacts,
    #[serde(default)]
    pub consumers_of_public_layout: Vec<Consumer>,
    #[serde(default)]
    pub checkpoints: Checkpoints,
}

#[derive(Debug, Deserialize)]
pub struct Circuit {
    pub name: String,
    pub package: String,
    pub entrypoint: String,
    pub tree_depth: u64,
    pub split_count: u64,
    pub nr_public_inputs: u64,
}

#[derive(Debug, Deserialize)]
pub struct PublicInput {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub bytes: u64,
    #[serde(default)]
    pub encoding: Option<String>,
    #[serde(default)]
    pub padded_to: Option<u64>,
    #[serde(default)]
    pub padding: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PrivateInput {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub element: Option<String>,
    #[serde(default)]
    pub length: Option<u64>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Constraint {
    pub id: String,
    pub formula: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HashFunctions {
    pub hash_1: HashFunction,
    pub hash_2: HashFunction,
    pub hash_3: HashFunction,
    pub hash_4: HashFunctionForbidden,
}

#[derive(Debug, Deserialize)]
pub struct HashFunction {
    pub signature: String,
    pub implementation: String,
}

#[derive(Debug, Deserialize)]
pub struct HashFunctionForbidden {
    pub status: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WitnessLayout {
    #[serde(default)]
    pub description: Option<String>,
    pub header: WitnessHeader,
    pub public_inputs_section: WitnessPublicSection,
    pub total_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub struct WitnessHeader {
    pub bytes: u64,
    #[serde(default)]
    pub fields: Vec<WitnessHeaderField>,
}

#[derive(Debug, Deserialize)]
pub struct WitnessHeaderField {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub endianness: Option<String>,
    #[serde(default)]
    pub value: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct WitnessPublicSection {
    pub bytes: u64,
    #[serde(default)]
    pub layout: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Artifacts {
    #[serde(default)]
    pub description: Option<String>,
    pub json: String,
    pub consumers: Consumers,
}

#[derive(Debug, Deserialize)]
pub struct Consumers {
    pub prover: ConsumerEntry,
    pub merkle: ConsumerEntry,
    pub web: ConsumerEntry,
}

#[derive(Debug, Deserialize)]
pub struct ConsumerEntry {
    pub mode: String,
    pub path: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Consumer {
    pub layer: String,
    pub path: String,
    #[serde(default)]
    pub function: Option<String>,
    pub obligation: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct Checkpoints {
    #[serde(default)]
    pub stage_2: Vec<String>,
    #[serde(default)]
    pub stage_3: Vec<String>,
}

impl Spec {
    /// Loads and parses `spec.json` from the given path.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let spec: Spec = serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse {} as spec", path.display()))?;
        Ok(spec)
    }
}

/// Prints a short summary of what the spec contains.
pub fn print_summary(spec: &Spec) -> Result<()> {
    println!();
    println!("=== Spec summary ===");
    println!("  version:            {}", spec.version);
    println!("  circuit name:       {}", spec.circuit.name);
    println!("  tree depth:         {}", spec.circuit.tree_depth);
    println!("  split count:        {}", spec.circuit.split_count);
    println!("  nr public inputs:   {}", spec.circuit.nr_public_inputs);
    println!("  public inputs:      {} entries", spec.public_inputs.len());
    println!(
        "  private inputs:     {} entries",
        spec.private_inputs.len()
    );
    println!(
        "  witness total:      {} bytes",
        spec.witness_layout.total_bytes
    );
    println!();

    println!("  Public inputs (in order):");
    for (i, pi) in spec.public_inputs.iter().enumerate() {
        let padded = pi
            .padded_to
            .map(|p| format!(", padded_to={}", p))
            .unwrap_or_default();
        println!(
            "    [{}] {} : {} ({} bytes{})",
            i, pi.name, pi.ty, pi.bytes, padded
        );
    }
    println!();

    println!("  Private inputs:");
    for (i, pi) in spec.private_inputs.iter().enumerate() {
        let extra = match (&pi.element, pi.length) {
            (Some(el), Some(len)) => format!(" [{}; {}]", el, len),
            _ => String::new(),
        };
        println!("    [{}] {} : {}{}", i, pi.name, pi.ty, extra);
    }
    println!();

    Ok(())
}
