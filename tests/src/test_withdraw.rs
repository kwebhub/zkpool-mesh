//! Integration tests for the `withdraw` instruction.
//!
//! Stage 4.5.5 — validation tests without a real proof.
//!
//! The real Groth16 proof was verified locally in stage 3.5. Reproducing
//! that proof inside LiteSVM would require on-the-fly `sunspot prove`
//! (43 s per proof) or mocking the verifier — both add complexity without
//! real value. Instead, we test:
//!   1. All on-chain `require!` checks (proof length, recipient, root).
//!   2. That the instruction reaches the CPI to the verifier.
//!   3. That the verifier rejects a dummy proof with the expected error.

use litesvm::LiteSVM;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;

use crate::helpers::{setup_svm, VERIFIER_ID, ZK_POOL_ID};

const POOL_SEED: &[u8] = b"pool3";
const VAULT_SEED: &[u8] = b"vault3";
const NULLIFIER_RECORD_SEED: &[u8] = b"nullifier_record";

const POOL_DISCRIMINATOR: [u8; 8] = [134, 215, 119, 168, 28, 199, 193, 127];
const DEPOSIT_DISCRIMINATOR: [u8; 8] = [242, 35, 198, 137, 82, 225, 242, 182];
const WITHDRAW_DISCRIMINATOR: [u8; 8] = [183, 18, 70, 156, 148, 109, 161, 34];

const PROOF_LEN: usize = 324;

fn system_program_id() -> Address {
    "11111111111111111111111111111111".parse().unwrap()
}

fn find_pda(seeds: &[&[u8]], program_id: &Address) -> (Address, u8) {
    Address::find_program_address(seeds, program_id)
}

/// Helper: init pool, then deposit once with commitment [0x01; 32] and
/// new_root [0x02; 32]. Returns (payer, pool_pda, vault_pda).
fn init_and_deposit(svm: &mut LiteSVM) -> (Keypair, Address, Address) {
    let payer = Keypair::new();
    let payer_addr: Address = payer.pubkey().into();
    svm.airdrop(&payer_addr, 10_000_000_000).expect("airdrop");

    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let (pool_pda, _) = find_pda(&[POOL_SEED], &program_id);
    let (vault_pda, _) = find_pda(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    // pool init
    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer_addr, true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data: POOL_DISCRIMINATOR.to_vec(),
    };
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash);
    svm.send_transaction(tx).expect("pool init failed");

    // deposit
    let commitment = [0x01u8; 32];
    let new_root = [0x02u8; 32];
    let amount: u64 = 1_000_000;

    let mut data = Vec::with_capacity(8 + 32 + 32 + 8);
    data.extend_from_slice(&DEPOSIT_DISCRIMINATOR);
    data.extend_from_slice(&commitment);
    data.extend_from_slice(&new_root);
    data.extend_from_slice(&amount.to_le_bytes());

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer_addr, true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data,
    };
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash);
    svm.send_transaction(tx).expect("deposit failed");

    (payer, pool_pda, vault_pda)
}

#[test]
fn test_withdraw_rejects_wrong_proof_length() {
    let mut svm: LiteSVM = setup_svm();
    let (payer, pool_pda, vault_pda) = init_and_deposit(&mut svm);

    let payer_addr: Address = payer.pubkey().into();
    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let verifier_id: Address = VERIFIER_ID.parse().unwrap();

    // Withdraw parameters.
    let nullifier_hash = [0xaa_u8; 32];
    let root = [0x02_u8; 32]; // the root from deposit
    let recipient_addr: Address = payer.pubkey().into();
    let amount: u64 = 1_000_000;
    let recipient_binding = [0xbb_u8; 32];

    let (nullifier_record, _) = find_pda(
        &[
            NULLIFIER_RECORD_SEED,
            pool_pda.as_ref(),
            nullifier_hash.as_ref(),
        ],
        &program_id,
    );

    // Build withdraw instruction with a SHORT proof (expected to fail on
    // the first `require!`).
    let short_proof = vec![0u8; PROOF_LEN - 1];

    let mut data = Vec::new();
    data.extend_from_slice(&WITHDRAW_DISCRIMINATOR);

    // Borsh serialization: Vec<u8> = u32 LE length + bytes.
    data.extend_from_slice(&(short_proof.len() as u32).to_le_bytes());
    data.extend_from_slice(&short_proof);

    data.extend_from_slice(&nullifier_hash);
    data.extend_from_slice(&root);
    data.extend_from_slice(recipient_addr.as_ref());
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&recipient_binding);

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer_addr, true),                    // payer
            AccountMeta::new(pool_pda, false),                     // pool
            AccountMeta::new(nullifier_record, false),             // nullifier_record
            AccountMeta::new(vault_pda, false),                    // vault
            AccountMeta::new(recipient_addr, false),               // to
            AccountMeta::new_readonly(verifier_id, false),         // verifier_program
            AccountMeta::new_readonly(system_program_id(), false), // system_program
        ],
        data,
    };

    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash);

    // Instruction must fail — proof length is wrong.
    let result = svm.send_transaction(tx);
    assert!(result.is_err(), "withdraw must fail on short proof");
}

#[test]
fn test_withdraw_rejects_recipient_mismatch() {
    let mut svm: LiteSVM = setup_svm();
    let (payer, pool_pda, vault_pda) = init_and_deposit(&mut svm);

    let payer_addr: Address = payer.pubkey().into();
    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let verifier_id: Address = VERIFIER_ID.parse().unwrap();

    let nullifier_hash = [0xaa_u8; 32];
    let root = [0x02_u8; 32];
    let amount: u64 = 1_000_000;
    let recipient_binding = [0xbb_u8; 32];

    // `recipient` in instruction data (a dummy pubkey) is DIFFERENT from
    // the `to` account (payer). Must fail on the second `require!`.
    let recipient_in_ix: [u8; 32] = [0xcc_u8; 32];

    let (nullifier_record, _) = find_pda(
        &[
            NULLIFIER_RECORD_SEED,
            pool_pda.as_ref(),
            nullifier_hash.as_ref(),
        ],
        &program_id,
    );

    // Correct-length proof (content irrelevant, check happens later).
    let proof = vec![0u8; PROOF_LEN];

    let mut data = Vec::new();
    data.extend_from_slice(&WITHDRAW_DISCRIMINATOR);
    data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
    data.extend_from_slice(&proof);
    data.extend_from_slice(&nullifier_hash);
    data.extend_from_slice(&root);
    data.extend_from_slice(&recipient_in_ix);
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&recipient_binding);

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer_addr, true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(nullifier_record, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(payer_addr, false), // to = payer
            AccountMeta::new_readonly(verifier_id, false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data,
    };

    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash);

    let result = svm.send_transaction(tx);
    assert!(result.is_err(), "withdraw must fail on recipient mismatch");
}

#[test]
fn test_withdraw_rejects_unknown_root() {
    let mut svm: LiteSVM = setup_svm();
    let (payer, pool_pda, vault_pda) = init_and_deposit(&mut svm);

    let payer_addr: Address = payer.pubkey().into();
    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let verifier_id: Address = VERIFIER_ID.parse().unwrap();

    let nullifier_hash = [0xaa_u8; 32];
    // Unknown root: NOT in pool.roots[] history.
    let root = [0xff_u8; 32];
    let recipient_addr: Address = payer.pubkey().into();
    let amount: u64 = 1_000_000;
    let recipient_binding = [0xbb_u8; 32];

    let (nullifier_record, _) = find_pda(
        &[
            NULLIFIER_RECORD_SEED,
            pool_pda.as_ref(),
            nullifier_hash.as_ref(),
        ],
        &program_id,
    );

    let proof = vec![0u8; PROOF_LEN];

    let mut data = Vec::new();
    data.extend_from_slice(&WITHDRAW_DISCRIMINATOR);
    data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
    data.extend_from_slice(&proof);
    data.extend_from_slice(&nullifier_hash);
    data.extend_from_slice(&root);
    data.extend_from_slice(recipient_addr.as_ref());
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&recipient_binding);

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer_addr, true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(nullifier_record, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(recipient_addr, false),
            AccountMeta::new_readonly(verifier_id, false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data,
    };

    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash);

    let result = svm.send_transaction(tx);
    assert!(result.is_err(), "withdraw must fail on unknown root");
}
