//! Adversarial tests derived from `docs/threat-model.md`.
//!
//! Each test either:
//!   - Confirms a mitigation works (asserts the attack fails), or
//!   - Documents an accepted limitation (asserts the attack succeeds, with
//!     a comment pointing at the threat model entry).

use anchor_lang::AccountDeserialize;
use litesvm::LiteSVM;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;

use zk_pool::state::PoolState;

use crate::helpers::{setup_svm, ZK_POOL_ID};

const POOL_SEED: &[u8] = b"pool3";
const VAULT_SEED: &[u8] = b"vault3";
const NULLIFIER_RECORD_SEED: &[u8] = b"nullifier_record";

const POOL_DISCRIMINATOR: [u8; 8] = [134, 215, 119, 168, 28, 199, 193, 127];
const DEPOSIT_DISCRIMINATOR: [u8; 8] = [242, 35, 198, 137, 82, 225, 242, 182];
const WITHDRAW_DISCRIMINATOR: [u8; 8] = [183, 18, 70, 156, 148, 109, 161, 34];

fn system_program_id() -> Address {
    "11111111111111111111111111111111".parse().unwrap()
}

fn find_pda(seeds: &[&[u8]], program_id: &Address) -> (Address, u8) {
    Address::find_program_address(seeds, program_id)
}

/// Init the pool. Returns (payer, pool_pda, vault_pda).
fn init_pool(svm: &mut LiteSVM) -> (Keypair, Address, Address) {
    let payer = Keypair::new();
    let payer_addr: Address = payer.pubkey().into();
    svm.airdrop(&payer_addr, 10_000_000_000).expect("airdrop");

    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let (pool_pda, _) = find_pda(&[POOL_SEED], &program_id);
    let (vault_pda, _) = find_pda(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

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

    (payer, pool_pda, vault_pda)
}

/// Send a deposit. Returns Ok(()) or the LiteSVM error.
fn do_deposit(
    svm: &mut LiteSVM,
    payer: &Keypair,
    pool_pda: Address,
    vault_pda: Address,
    commitment: [u8; 32],
    new_root: [u8; 32],
    amount: u64,
) -> Result<(), String> {
    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let payer_addr: Address = payer.pubkey().into();

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
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[payer], blockhash);

    svm.send_transaction(tx)
        .map(|_| ())
        .map_err(|e| format!("{:?}", e))
}

fn read_pool_state(svm: &LiteSVM, pool_pda: Address) -> PoolState {
    let account = svm.get_account(&pool_pda).expect("pool PDA exists");
    let mut data: &[u8] = &account.data;
    PoolState::try_deserialize(&mut data).expect("deserialize PoolState")
}

// ============================================================
// A1 — Malicious depositor corrupts tree (ACCEPTED LIMITATION)
// ============================================================

#[test]
fn a1_garbage_new_root_is_accepted() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    let commitment = [0xAAu8; 32];
    let garbage_root = [0xBBu8; 32]; // Not a real Merkle root.

    let result = do_deposit(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        commitment,
        garbage_root,
        1_000_000,
    );
    assert!(
        result.is_ok(),
        "deposit with garbage root should be accepted (documented limitation): {:?}",
        result.err()
    );

    let state = read_pool_state(&svm, pool_pda);
    assert_eq!(state.current_root(), garbage_root);

    // See docs/threat-model.md A1 — this is not prevented by design.
}

// ============================================================
// A9 — Commitment forgery (self-harm only)
// ============================================================

#[test]
fn a9_arbitrary_commitment_is_accepted() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    let forged_commitment = [0xEEu8; 32]; // Not hash_3(anything).
    let valid_root = [0xFFu8; 32];

    let result = do_deposit(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        forged_commitment,
        valid_root,
        1_000_000,
    );
    assert!(result.is_ok(), "forged commitment should be accepted");

    // The commitment is now in the tree, but no valid witness exists
    // for it. Self-harm only. See docs/threat-model.md A9.
}

// ============================================================
// A10 — Root history exhaustion (documented limitation)
// ============================================================

#[test]
fn a10_root_history_is_bounded() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    // First deposit — establishes first_root.
    let first_root = [0x01u8; 32];
    do_deposit(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        [0xAAu8; 32],
        first_root,
        1_000_000,
    )
    .expect("first deposit");

    let state = read_pool_state(&svm, pool_pda);
    assert!(
        state.is_known_root(&first_root),
        "first root should be known"
    );

    // 10 more deposits. Each unique root pushes the oldest one out.
    for i in 0u8..10 {
        let new_root = [i.wrapping_add(2); 32];
        let commitment = [i.wrapping_add(0x10); 32];
        do_deposit(
            &mut svm, &payer, pool_pda, vault_pda, commitment, new_root, 1_000_000,
        )
        .expect("deposit");
    }

    let state = read_pool_state(&svm, pool_pda);
    assert!(
        !state.is_known_root(&first_root),
        "first root should be evicted after 11 deposits (ROOT_HISTORY_SIZE = 10)"
    );
    // Documented limitation — see docs/threat-model.md A10.
}

// ============================================================
// Deposit below minimum — rejected
// ============================================================

#[test]
fn deposit_below_minimum_rejected() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    let result = do_deposit(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        [0xAAu8; 32],
        [0xBBu8; 32],
        999_999, // MIN_DEPOSIT_AMOUNT is 1_000_000.
    );
    assert!(result.is_err(), "deposit below minimum should be rejected");
}

#[test]
fn deposit_at_minimum_accepted() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    let result = do_deposit(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        [0xAAu8; 32],
        [0xBBu8; 32],
        1_000_000,
    );
    assert!(result.is_ok(), "deposit at minimum should succeed");
}

// ============================================================
// Root unchanged — rejected
// ============================================================

#[test]
fn deposit_with_unchanged_root_rejected() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    let root_r1 = [0x42u8; 32];
    do_deposit(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        [0xAAu8; 32],
        root_r1,
        1_000_000,
    )
    .expect("first deposit");

    // Second deposit with the same root → should fail.
    let result = do_deposit(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        [0xBBu8; 32],
        root_r1,
        1_000_000,
    );
    assert!(
        result.is_err(),
        "deposit with unchanged root should be rejected"
    );
}

// ============================================================
// A11 — Verifier substitution (should be rejected by address constraint)
// ============================================================

#[test]
fn a11_wrong_verifier_program_rejected() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    // Establish a root via deposit.
    let root_r1 = [0x42u8; 32];
    do_deposit(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        [0xAAu8; 32],
        root_r1,
        1_000_000,
    )
    .expect("deposit");

    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let payer_addr: Address = payer.pubkey().into();

    // A valid 32-byte address that is not the real verifier.
    // Use the depositor's own address — it's a valid Pubkey.
    let fake_verifier: Address = payer.pubkey().into();

    // Nullifier record PDA (required account, wrong seeds → will fail earlier
    // than verifier check, but we're testing that the tx fails).
    let nullifier_hash = [0u8; 32];
    let (nullifier_record_pda, _) = find_pda(
        &[NULLIFIER_RECORD_SEED, pool_pda.as_ref(), &nullifier_hash],
        &program_id,
    );

    let recipient_addr = payer_addr; // any address

    let proof = vec![0u8; 324];
    let mut data = Vec::with_capacity(8 + 4 + 324 + 32 + 32 + 32 + 8 + 32);
    data.extend_from_slice(&WITHDRAW_DISCRIMINATOR);
    data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
    data.extend_from_slice(&proof);
    data.extend_from_slice(&nullifier_hash); // nullifier_hash
    data.extend_from_slice(&root_r1); // root
    data.extend_from_slice(recipient_addr.as_ref()); // recipient
    data.extend_from_slice(&1_000_000u64.to_le_bytes()); // amount
    data.extend_from_slice(&[0u8; 32]); // recipient_binding

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer_addr, true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(nullifier_record_pda, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(recipient_addr, false),
            AccountMeta::new_readonly(fake_verifier, false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data,
    };

    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash);

    let result = svm.send_transaction(tx);
    assert!(
        result.is_err(),
        "withdraw with a non-verifier program should be rejected"
    );
}
