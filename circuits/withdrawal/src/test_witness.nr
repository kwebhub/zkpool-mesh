//! Test-only module: generates a valid witness for the withdrawal circuit
//! and prints it in `Prover.toml` format.
//!
//! Usage:
//!   cd circuits/withdrawal
//!   nargo test test_generate_valid_inputs --show-output
//!
//! Copy the output between `=== Prover.toml ===` and `=== End ===` into
//! `Prover.toml`. That file is gitignored and is consumed by `nargo execute`
//! to produce a witness for `sunspot prove`.
//!
//! IMPORTANT: this module is compiled into the circuit. Any change here
//! changes the ACIR, and therefore CCS/PK/VK/.so and the verifier `.so`.
//! Keep the logic minimal and stable.
//!
//! NOTE: Field values must be < 2^254 (BN254 prime). All literals below are
//! chosen small enough to fit.
//!
//! Stage 15: the witness now includes `splits` (3 field elements) and
//! `note_index` (u32). The legacy witness format (without splits) is no
//! longer supported — see `docs/notes/15-split-deposit.md`.

use crate::compute_merkle_root;
use poseidon::{hash_1, hash_2, hash_3};

/// Builds a valid witness for a 20-level tree with a 3-way split,
/// and prints it in `Prover.toml` format. The tree is synthetic — we
/// don't need it to be canonical, we only need the root to be internally
/// consistent so the constraints pass.
#[test]
fn test_generate_valid_inputs() {
    // --- private inputs (per-note secrets) ---
    let nullifier: Field = 111111111111111111;
    let secret: Field = 222222222222222222;
    let note_secret: Field = 333333333333333333;

    // --- split vector (Stage 15) ---
    // 3-way split of 1_000_000 lamports:
    //   splits[0] = 500_000
    //   splits[1] = 300_000
    //   splits[2] = 200_000
    // This note is `splits[1]`, so amount = 300_000.
    let split_0: Field = 500_000;
    let split_1: Field = 300_000;
    let split_2: Field = 200_000;
    let splits: [Field; 3] = [split_0, split_1, split_2];
    let note_index: u32 = 1;

    let amount: Field = split_1;               // per-note amount
    let total_amount: Field = 1_000_000;       // aggregate deposit amount

    // --- derive commitment, nullifier_hash, recipient_binding ---
    let commitment = hash_3(nullifier, secret, amount);
    let nullifier_hash = hash_1(nullifier);

    let recipient: Field = 444444444444444444;
    let recipient_binding = hash_2(note_secret, recipient);

    // --- build synthetic 20-level Merkle tree ---
    let s0: Field = 555555555555555555;

    let mut merkle_proof: [Field; 20] = [0; 20];
    let mut is_even: [bool; 20] = [false; 20];

    merkle_proof[0] = s0;
    is_even[0] = true;
    for i in 1..20 {
        merkle_proof[i] = 0;
        is_even[i] = true;
    }

    let root = compute_merkle_root::<20>(commitment, merkle_proof, is_even);

    // --- print Prover.toml (markers only; content parsed by a host script) ---
    // Order matches `spec.json` public_inputs[] followed by private_inputs[].
    println("=== Prover.toml ===");
    // public (6)
    println(root);
    println(nullifier_hash);
    println(recipient);
    println(recipient_binding);
    println(amount);
    println(total_amount);
    // private (7) — scalars first, then arrays
    println(nullifier);
    println(secret);
    println(note_secret);

    // merkle_proof: 20 field elements, one per line
    for i in 0..20 {
        println(merkle_proof[i]);
    }

    // is_even: 20 booleans, printed as 1/0
    for i in 0..20 {
        if is_even[i] {
            println(1);
        } else {
            println(0);
        }
    }

    // splits: 3 field elements, one per line
    for i in 0..3 {
        println(splits[i]);
    }

    // note_index: u32, printed as a decimal integer
    println(note_index);
    println("=== End ===");
}
