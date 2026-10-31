//! Integration tests for the `deposit_split` instruction (Stage 15).
//!
//! Covers:
//!   - Happy path: three commitments, one final root.
//!   - Reject cases: sum mismatch, per-split below minimum, unchanged root,
//!     sum overflow.
//!   - State assertions: next_leaf_index advances by 3, total_deposits by 1,
//!     only the final root is stored in the history.
//!
//! ## Amount scale
//!
//! `MIN_DEPOSIT_AMOUNT = 1_000_000` lamports (0.001 SOL) applies to **each**
//! individual split, not to the total. Test amounts must therefore be
//! comfortably above 0.001 SOL per split. We use a 0.5 + 0.3 + 0.2 = 1.0 SOL
//! split, i.e. `[500_000_000, 300_000_000, 200_000_000]` lamports.

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

const POOL_DISCRIMINATOR: [u8; 8] = [134, 215, 119, 168, 28, 199, 193, 127];
const DEPOSIT_SPLIT_DISCRIMINATOR: [u8; 8] = [0x32, 0x11, 0x82, 0x11, 0x77, 0xdc, 0xae, 0x76];

fn system_program_id() -> Address {
    "11111111111111111111111111111111".parse().unwrap()
}

fn find_pda(seeds: &[&[u8]], program_id: &Address) -> (Address, u8) {
    Address::find_program_address(seeds, program_id)
}

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

/// Serialize `DepositSplitArgs` per Anchor/Borsh:
///   - 3 commitments: 32 bytes each, raw, no length prefix
///   - 3 new_roots:   32 bytes each, raw
///   - 3 amounts:     u64 LE each
///   - 1 total_amount: u64 LE
///
/// No `Vec` fields, so no length prefix. Structs are inlined in arg order.
#[allow(clippy::too_many_arguments)]
fn do_deposit_split(
    svm: &mut LiteSVM,
    payer: &Keypair,
    pool_pda: Address,
    vault_pda: Address,
    commitments: [[u8; 32]; 3],
    new_roots: [[u8; 32]; 3],
    amounts: [u64; 3],
    total_amount: u64,
) -> Result<(), String> {
    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let payer_addr: Address = payer.pubkey().into();

    let mut data = Vec::with_capacity(8 + 3 * 32 + 3 * 32 + 3 * 8 + 8);
    data.extend_from_slice(&DEPOSIT_SPLIT_DISCRIMINATOR);
    for c in &commitments {
        data.extend_from_slice(c);
    }
    for r in &new_roots {
        data.extend_from_slice(r);
    }
    for a in &amounts {
        data.extend_from_slice(&a.to_le_bytes());
    }
    data.extend_from_slice(&total_amount.to_le_bytes());

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

/// Fixed test data for a valid 3-way split: 0.5 + 0.3 + 0.2 = 1.0 SOL.
/// Each split is well above `MIN_DEPOSIT_AMOUNT` (0.001 SOL = 1_000_000 lamports).
fn valid_split() -> ([[u8; 32]; 3], [[u8; 32]; 3], [u64; 3], u64) {
    let commitments = [[0xAAu8; 32], [0xBBu8; 32], [0xCCu8; 32]];
    let new_roots = [[0x11u8; 32], [0x22u8; 32], [0x33u8; 32]];
    let amounts = [500_000_000u64, 300_000_000, 200_000_000];
    let total_amount = 1_000_000_000u64;
    (commitments, new_roots, amounts, total_amount)
}

// ============================================================
// Happy path
// ============================================================

#[test]
fn deposit_split_happy_path() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    let vault_initial = svm.get_balance(&vault_pda).expect("vault exists");
    assert!(vault_initial > 0, "vault must be rent-exempt after init");

    let (commitments, new_roots, amounts, total_amount) = valid_split();

    let result = do_deposit_split(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        commitments,
        new_roots,
        amounts,
        total_amount,
    );
    assert!(result.is_ok(), "deposit_split failed: {:?}", result.err());

    // Vault received exactly total_amount.
    let vault_after = svm.get_balance(&vault_pda).expect("vault exists");
    assert_eq!(
        vault_after,
        vault_initial + total_amount,
        "vault must increase by total_amount"
    );

    let state = read_pool_state(&svm, pool_pda);

    // next_leaf_index advanced by 3.
    assert_eq!(
        state.next_leaf_index, 3,
        "next_leaf_index must advance by SPLIT_COUNT"
    );

    // total_deposits advanced by 1 (one deposit transaction).
    assert_eq!(
        state.total_deposits, 1,
        "total_deposits must advance by 1 per call"
    );

    // Only the final root is in the history.
    let final_root = new_roots[2];
    assert!(
        state.is_known_root(&final_root),
        "final root must be in history"
    );
    // Intermediate roots are NOT stored.
    assert!(
        !state.is_known_root(&new_roots[0]),
        "intermediate root 0 must NOT be in history"
    );
    assert!(
        !state.is_known_root(&new_roots[1]),
        "intermediate root 1 must NOT be in history"
    );
}

// ============================================================
// Reject: sum mismatch
// ============================================================

#[test]
fn deposit_split_sum_mismatch_rejected() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    let (commitments, new_roots, amounts, mut total_amount) = valid_split();
    // All splits are individually valid; only the sum is wrong.
    total_amount -= 1;

    let result = do_deposit_split(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        commitments,
        new_roots,
        amounts,
        total_amount,
    );
    assert!(
        result.is_err(),
        "sum mismatch (Σ amounts != total_amount) must be rejected"
    );

    // State unchanged.
    let state = read_pool_state(&svm, pool_pda);
    assert_eq!(state.next_leaf_index, 0);
    assert_eq!(state.total_deposits, 0);
}

// ============================================================
// Reject: any individual split below minimum
// ============================================================

#[test]
fn deposit_split_below_minimum_rejected() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    let commitments = [[0xAAu8; 32], [0xBBu8; 32], [0xCCu8; 32]];
    let new_roots = [[0x11u8; 32], [0x22u8; 32], [0x33u8; 32]];
    // Third split is below MIN_DEPOSIT_AMOUNT (0.001 SOL = 1_000_000 lamports).
    let amounts = [500_000_000u64, 500_000_000, 100]; // third is tiny
    let total_amount = 1_000_000_100u64;

    let result = do_deposit_split(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        commitments,
        new_roots,
        amounts,
        total_amount,
    );
    assert!(
        result.is_err(),
        "any split below MIN_DEPOSIT_AMOUNT must be rejected"
    );
}

// ============================================================
// Reject: unchanged final root
// ============================================================

#[test]
fn deposit_split_unchanged_root_rejected() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    // First split establishes a root.
    let (commitments_1, new_roots_1, amounts_1, total_amount_1) = valid_split();
    do_deposit_split(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        commitments_1,
        new_roots_1,
        amounts_1,
        total_amount_1,
    )
    .expect("first split");

    // Second split reuses the same final root → rejected.
    let commitments_2 = [[0xDDu8; 32], [0xEEu8; 32], [0xFFu8; 32]];
    let new_roots_2 = [[0x44u8; 32], [0x55u8; 32], new_roots_1[2]];
    let amounts_2 = [500_000_000u64, 300_000_000, 200_000_000];
    let total_amount_2 = 1_000_000_000u64;

    let result = do_deposit_split(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        commitments_2,
        new_roots_2,
        amounts_2,
        total_amount_2,
    );
    assert!(result.is_err(), "unchanged final root must be rejected");

    // State from the first split is intact.
    let state = read_pool_state(&svm, pool_pda);
    assert_eq!(state.next_leaf_index, 3);
    assert_eq!(state.total_deposits, 1);
}

// ============================================================
// Reject: sum overflow in checked_add
// ============================================================

#[test]
fn deposit_split_sum_overflow_rejected() {
    let mut svm = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    let commitments = [[0xAAu8; 32], [0xBBu8; 32], [0xCCu8; 32]];
    let new_roots = [[0x11u8; 32], [0x22u8; 32], [0x33u8; 32]];
    // Sum overflows u64 → SplitSumMismatch (checked_add returns None).
    let amounts = [u64::MAX, u64::MAX, u64::MAX];
    let total_amount = u64::MAX;

    let result = do_deposit_split(
        &mut svm,
        &payer,
        pool_pda,
        vault_pda,
        commitments,
        new_roots,
        amounts,
        total_amount,
    );
    assert!(
        result.is_err(),
        "sum overflow must be rejected with SplitSumMismatch"
    );
}
