//! Groth16 proof generation via nargo + sunspot subprocesses.
//!
//! ## Why a mutex
//!
//! `sunspot prove` always writes `target/withdrawal.proof` and
//! `target/withdrawal.pw` — named after the ACIR, not the witness. Two
//! concurrent proofs would clobber each other. All proof generation is
//! serialized with an async mutex.
//!
//! ## Pipeline (per request, under mutex)
//!
//! 1. Write `<circuit_dir>/Prover-<uuid>.toml`.
//! 2. `nargo execute -p Prover-<uuid> w-<uuid>` → `<circuit_dir>/target/w-<uuid>.gz`.
//! 3. `sunspot prove <acir> <witness> <ccs> <pk>` → `withdrawal.proof` + `withdrawal.pw`.
//! 4. Read `.proof` + `.pw` as bytes.
//! 5. Clean up `Prover-<uuid>.toml` and `target/w-<uuid>.gz`.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;
use uuid::Uuid;

use crate::config::Config;
use crate::witness::WitnessInputs;

/// Result of a proof generation.
#[derive(Debug, Clone)]
pub struct ProofResult {
    /// Groth16 proof bytes (from `withdrawal.proof`).
    pub proof: Vec<u8>,
    /// Public witness bytes (from `withdrawal.pw`).
    pub public_witness: Vec<u8>,
}

/// Prover: holds config + serialization mutex.
pub struct Prover {
    config: Config,
    lock: Mutex<()>,
}

impl Prover {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            lock: Mutex::new(()),
        }
    }

    /// Generate a proof. Serialized by internal mutex.
    pub async fn prove(&self, inputs: &WitnessInputs) -> Result<ProofResult> {
        inputs.validate()?;

        let _guard = self.lock.lock().await;

        let uuid = Uuid::new_v4().simple().to_string();
        let prover_name = format!("Prover-{}", uuid);
        let witness_name = format!("w-{}", uuid);

        let circuit_dir = PathBuf::from(&self.config.circuit_dir);
        let prover_toml_path = circuit_dir.join(format!("{}.toml", prover_name));
        let witness_gz_path = circuit_dir
            .join("target")
            .join(format!("{}.gz", witness_name));

        // Ensure cleanup on every exit path.
        let result = self
            .run_pipeline(inputs, &prover_toml_path, &witness_name)
            .await;

        // Cleanup — best effort.
        let _ = tokio::fs::remove_file(&prover_toml_path).await;
        let _ = tokio::fs::remove_file(&witness_gz_path).await;

        result
    }

    async fn run_pipeline(
        &self,
        inputs: &WitnessInputs,
        prover_toml_path: &PathBuf,
        witness_name: &str,
    ) -> Result<ProofResult> {
        // 1. Write Prover TOML.
        let toml = inputs.to_toml()?;
        tokio::fs::write(prover_toml_path, toml)
            .await
            .with_context(|| format!("writing {}", prover_toml_path.display()))?;

        // 2. nargo execute.
        let prover_name = prover_toml_path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("invalid prover file name")?;

        self.run_nargo(prover_name, witness_name).await?;

        // 3. sunspot prove.
        self.run_sunspot(witness_name).await?;

        // 4. Read outputs.
        let proof = tokio::fs::read(self.config.proof_path())
            .await
            .context("reading proof file")?;
        let public_witness = tokio::fs::read(self.config.pw_path())
            .await
            .context("reading public witness file")?;

        Ok(ProofResult {
            proof,
            public_witness,
        })
    }

    async fn run_nargo(&self, prover_name: &str, witness_name: &str) -> Result<()> {
        let dur = Duration::from_secs(self.config.nargo_timeout_secs);

        let fut = Command::new(&self.config.nargo_bin)
            .arg("execute")
            .arg("-p")
            .arg(prover_name)
            .arg(witness_name)
            .current_dir(&self.config.circuit_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        let out = timeout(dur, fut)
            .await
            .context("nargo execute timed out")?
            .context("running nargo execute")?;

        if !out.status.success() {
            bail!(
                "nargo execute failed (status {:?}):\nstdout: {}\nstderr: {}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            );
        }
        Ok(())
    }

    async fn run_sunspot(&self, witness_name: &str) -> Result<()> {
        let dur = Duration::from_secs(self.config.sunspot_timeout_secs);
        let witness_gz = format!("target/{}.gz", witness_name);

        let fut = Command::new(&self.config.sunspot_bin)
            .arg("prove")
            .arg(self.config.acir_path())
            .arg(&witness_gz)
            .arg(self.config.ccs_path())
            .arg(self.config.pk_path())
            .current_dir(&self.config.circuit_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        let out = timeout(dur, fut)
            .await
            .context("sunspot prove timed out")?
            .context("running sunspot prove")?;

        if !out.status.success() {
            bail!(
                "sunspot prove failed (status {:?}):\nstdout: {}\nstderr: {}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            port: 4002,
            circuit_dir: "/home/ubuntu/circuits/withdrawal".to_string(),
            nargo_bin: "nargo".to_string(),
            sunspot_bin: "sunspot".to_string(),
            nargo_timeout_secs: 60,
            sunspot_timeout_secs: 60,
        }
    }

    fn sample_inputs() -> WitnessInputs {
        WitnessInputs {
            root: "0178bf57a93031d2ebc274f5134fff54be52fa57cb5e5f1052ce2fd7bac3bf80".to_string(),
            nullifier_hash: "1412cc9d862599e6869a1881c8562062b98537456d9035819288219b5cd3e6e4"
                .to_string(),
            recipient: "062afbde1181c71c".to_string(),
            recipient_binding: "200cfdb247b0436ba0483327abcf8f83ed65f75a0db8d9ebbb611561d7d3b2e1"
                .to_string(),
            amount: "0493e0".to_string(),
            total_amount: "0f4240".to_string(),
            nullifier: "018abef7846071c7".to_string(),
            secret: "03157def08c0e38e".to_string(),
            note_secret: "04a03ce68d215555".to_string(),
            merkle_proof: std::iter::once("07b5bad595e238e3".to_string())
                .chain(std::iter::repeat("00".to_string()).take(19))
                .collect(),
            is_even: vec![true; 20],
            splits: vec![
                "07a120".to_string(),
                "0493e0".to_string(),
                "030d40".to_string(),
            ],
            note_index: 1,
        }
    }

    #[tokio::test]
    #[ignore = "requires nargo + sunspot + circuit artifacts"]
    async fn test_prove_real() {
        let prover = Prover::new(test_config());
        let r = prover.prove(&sample_inputs()).await.expect("prove");
        // Stage 15.4: proof is 388 B (was 324), public witness 204 B (was 172).
        assert_eq!(r.proof.len(), 388);
        assert_eq!(r.public_witness.len(), 204);
    }
}
