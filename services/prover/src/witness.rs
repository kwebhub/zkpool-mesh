//! Serialization of withdrawal witness inputs into `Prover.toml`.
//!
//! `nargo execute -p <name>` reads `<circuit_dir>/<name>.toml`. All hex
//! field values MUST be `0x`-prefixed (verified: bare hex is rejected by
//! the nargo TOML deserializer for `amount` and `merkle_proof` elements).
//!
//! ## Stage 15 (split deposit)
//!
//! The circuit now has 6 public inputs (was 5) and 7 private inputs (was 5).
//! `to_toml()` writes them in the exact order produced by
//! `circuits/withdrawal/src/test_witness.nr` — this is the order `nargo
//! execute` expects:
//!
//!   1. root              (public)
//!   2. nullifier_hash    (public)
//!   3. recipient         (public)
//!   4. recipient_binding (public)
//!   5. amount            (public, this note's amount)
//!   6. total_amount      (public, aggregate deposit amount)   ← NEW
//!   7. nullifier         (private)
//!   8. secret            (private)
//!   9. note_secret       (private)
//!  10. merkle_proof[20]  (private)
//!  11. is_even[20]       (private)
//!  12. splits[3]         (private)                             ← NEW
//!  13. note_index        (private, u32)                        ← NEW
//!
//! Legacy single-commitment notes (created via `deposit`, not
//! `deposit_split`) carry a `splits` array of length 1. Both shapes are
//! accepted: length-1 vectors are treated as `total_amount == amount`.

use anyhow::{bail, Result};

/// Merkle tree depth — must match the circuit.
pub const TREE_DEPTH: usize = 20;

/// Number of splits — must match the circuit (`SPLIT_COUNT`).
pub const SPLIT_COUNT: usize = 3;

/// Withdrawal witness inputs, as received over HTTP.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct WitnessInputs {
    // ---- public ----
    pub root: String,
    pub nullifier_hash: String,
    pub recipient: String,
    pub recipient_binding: String,
    pub amount: String,
    pub total_amount: String,
    // ---- private ----
    pub nullifier: String,
    pub secret: String,
    pub note_secret: String,
    pub merkle_proof: Vec<String>,
    pub is_even: Vec<bool>,
    pub splits: Vec<String>,
    pub note_index: u32,
}

impl WitnessInputs {
    /// Validate shape and length. Called before serialization.
    pub fn validate(&self) -> Result<()> {
        if self.merkle_proof.len() != TREE_DEPTH {
            bail!(
                "merkle_proof must have {} elements, got {}",
                TREE_DEPTH,
                self.merkle_proof.len()
            );
        }
        if self.is_even.len() != TREE_DEPTH {
            bail!(
                "is_even must have {} elements, got {}",
                TREE_DEPTH,
                self.is_even.len()
            );
        }
        if self.splits.len() != 1 && self.splits.len() != SPLIT_COUNT {
            bail!(
                "splits must have 1 or {} elements, got {}",
                SPLIT_COUNT,
                self.splits.len()
            );
        }
        if self.note_index as usize >= self.splits.len() {
            bail!(
                "note_index must be < splits.len() ({}), got {}",
                self.splits.len(),
                self.note_index
            );
        }
        for (name, v) in [
            ("root", &self.root),
            ("nullifier_hash", &self.nullifier_hash),
            ("recipient", &self.recipient),
            ("recipient_binding", &self.recipient_binding),
            ("amount", &self.amount),
            ("total_amount", &self.total_amount),
            ("nullifier", &self.nullifier),
            ("secret", &self.secret),
            ("note_secret", &self.note_secret),
        ] {
            if v.is_empty() {
                bail!("{} must not be empty", name);
            }
        }
        for (i, s) in self.splits.iter().enumerate() {
            if s.is_empty() {
                bail!("splits[{}] must not be empty", i);
            }
        }
        Ok(())
    }

    /// Render as TOML text for `nargo execute`.
    ///
    /// All hex values are `0x`-prefixed, because nargo's TOML deserializer
    /// rejects bare hex for `amount` and `merkle_proof[i]`.
    ///
    /// Order matches `circuits/withdrawal/src/test_witness.nr`.
    pub fn to_toml(&self) -> Result<String> {
        self.validate()?;

        let mut s = String::new();

        // ---- public (6) ----
        s.push_str(&format!("root = \"{}\"\n", with_0x(&self.root)));
        s.push_str(&format!(
            "nullifier_hash = \"{}\"\n",
            with_0x(&self.nullifier_hash)
        ));
        s.push_str(&format!("recipient = \"{}\"\n", with_0x(&self.recipient)));
        s.push_str(&format!(
            "recipient_binding = \"{}\"\n",
            with_0x(&self.recipient_binding)
        ));
        s.push_str(&format!("amount = \"{}\"\n", with_0x(&self.amount)));
        s.push_str(&format!(
            "total_amount = \"{}\"\n",
            with_0x(&self.total_amount)
        ));

        // ---- private: scalars ----
        s.push_str(&format!("nullifier = \"{}\"\n", with_0x(&self.nullifier)));
        s.push_str(&format!("secret = \"{}\"\n", with_0x(&self.secret)));
        s.push_str(&format!(
            "note_secret = \"{}\"\n",
            with_0x(&self.note_secret)
        ));

        // ---- private: arrays ----
        s.push_str("merkle_proof = [");
        for (i, v) in self.merkle_proof.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("\"{}\"", with_0x(v)));
        }
        s.push_str("]\n");

        s.push_str("is_even = [");
        for (i, v) in self.is_even.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(if *v { "true" } else { "false" });
        }
        s.push_str("]\n");

        s.push_str("splits = [");
        for (i, v) in self.splits.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("\"{}\"", with_0x(v)));
        }
        s.push_str("]\n");

        s.push_str(&format!("note_index = {}\n", self.note_index));

        Ok(s)
    }
}

/// Ensure a hex string has the `0x` prefix.
fn with_0x(s: &str) -> String {
    if s.starts_with("0x") || s.starts_with("0X") {
        s.to_string()
    } else {
        format!("0x{}", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> WitnessInputs {
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
            is_even: vec![true; TREE_DEPTH],
            splits: vec![
                "07a120".to_string(),
                "0493e0".to_string(),
                "030d40".to_string(),
            ],
            note_index: 1,
        }
    }

    #[test]
    fn test_validate_ok() {
        sample().validate().expect("valid");
    }

    #[test]
    fn test_validate_single_split_ok() {
        let mut w = sample();
        w.splits = vec!["07a120".to_string()];
        w.note_index = 0;
        w.validate().expect("legacy single split is valid");
    }

    #[test]
    fn test_validate_wrong_merkle_len() {
        let mut w = sample();
        w.merkle_proof.pop();
        assert!(w.validate().is_err());
    }

    #[test]
    fn test_validate_wrong_is_even_len() {
        let mut w = sample();
        w.is_even.pop();
        assert!(w.validate().is_err());
    }

    #[test]
    fn test_validate_two_splits_rejected() {
        let mut w = sample();
        w.splits = vec!["07a120".to_string(), "0493e0".to_string()];
        assert!(w.validate().is_err());
    }

    #[test]
    fn test_validate_note_index_out_of_bounds() {
        let mut w = sample();
        w.note_index = 5;
        assert!(w.validate().is_err());
    }

    #[test]
    fn test_validate_note_index_ok_for_single_split() {
        let mut w = sample();
        w.splits = vec!["07a120".to_string()];
        w.note_index = 0;
        w.validate().expect("index 0 valid for length-1 vector");
    }

    #[test]
    fn test_to_toml_has_0x_prefix() {
        let t = sample().to_toml().expect("toml");
        assert!(t.contains("root = \"0x0178"));
        assert!(t.contains("amount = \"0x0493e0\""));
        assert!(t.contains("total_amount = \"0x0f4240\""));
        assert!(t.contains("merkle_proof = [\"0x07b5bad595e238e3\""));
        assert!(t.contains("splits = [\"0x07a120\", \"0x0493e0\", \"0x030d40\"]"));
        assert!(t.contains("note_index = 1\n"));
    }

    #[test]
    fn test_to_toml_idempotent_0x() {
        let mut w = sample();
        w.root = "0x0178bf57a93031d2ebc274f5134fff54be52fa57cb5e5f1052ce2fd7bac3bf80".to_string();
        let t = w.to_toml().expect("toml");
        assert!(t.contains("root = \"0x0178"));
        assert!(!t.contains("0x0x"));
    }

    #[test]
    fn test_to_toml_lists_lengths() {
        let t = sample().to_toml().expect("toml");
        let proof_line = t.lines().find(|l| l.starts_with("merkle_proof")).unwrap();
        assert_eq!(proof_line.matches("\"0x").count(), TREE_DEPTH);
        let is_even_line = t.lines().find(|l| l.starts_with("is_even")).unwrap();
        assert_eq!(is_even_line.matches("true").count(), TREE_DEPTH);
        let splits_line = t.lines().find(|l| l.starts_with("splits")).unwrap();
        assert_eq!(splits_line.matches("\"0x").count(), SPLIT_COUNT);
    }

    #[test]
    fn test_to_toml_all_thirteen_fields() {
        let t = sample().to_toml().expect("toml");
        // 6 public + 3 private scalars + merkle_proof + is_even + splits + note_index = 13 lines
        assert_eq!(t.lines().count(), 13);
    }
}
