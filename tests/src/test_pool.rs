//! Integration tests for the `pool` instruction.
//!
//! Stage 4.5.3 — full test: build and submit a `pool` transaction, then
//! verify the resulting on-chain state.

use anchor_lang::AccountDeserialize;
use litesvm::LiteSVM;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;

use zk_pool::state::PoolState;

use crate::helpers::{setup_svm, ZK_POOL_ID};

/// Seed for the `PoolState` PDA — matches `zk_pool::constants::POOL_SEED`.
const POOL_SEED: &[u8] = b"pool3";

/// Seed for the vault PDA.
const VAULT_SEED: &[u8] = b"vault3";

/// Discriminator for the `pool` instruction (from the generated IDL).
const POOL_DISCRIMINATOR: [u8; 8] = [134, 215, 119, 168, 28, 199, 193, 127];

/// System program ID.
fn system_program_id() -> Address {
    "11111111111111111111111111111111".parse().unwrap()
}

/// Derives a PDA using `solana-address`'s helper.
fn find_pda(seeds: &[&[u8]], program_id: &Address) -> (Address, u8) {
    Address::find_program_address(seeds, program_id)
}

#[test]
fn test_airdrop_works() {
    let mut svm: LiteSVM = setup_svm();
    let payer = Keypair::new();

    let payer_addr: Address = payer.pubkey().into();

    assert_eq!(svm.get_balance(&payer_addr), None);

    let lamports = 10_000_000_000u64;
    svm.airdrop(&payer_addr, lamports).expect("airdrop failed");

    assert_eq!(svm.get_balance(&payer_addr), Some(lamports));
}

#[test]
fn test_pool_creates_state_and_vault() {
    let mut svm: LiteSVM = setup_svm();

    // --- payer ---
    let payer = Keypair::new();
    let payer_addr: Address = payer.pubkey().into();
    svm.airdrop(&payer_addr, 10_000_000_000)
        .expect("airdrop failed");

    // --- PDAs ---
    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let (pool_pda, _pool_bump) = find_pda(&[POOL_SEED], &program_id);
    let (vault_pda, _vault_bump) = find_pda(&[VAULT_SEED, pool_pda.as_ref()], &program_id);

    // Initially, neither PDA exists.
    assert!(
        svm.get_account(&pool_pda).is_none(),
        "pool PDA must not exist yet"
    );
    assert!(
        svm.get_account(&vault_pda).is_none(),
        "vault PDA must not exist yet"
    );

    // --- build instruction ---
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

    // --- build and sign transaction ---
    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash);

    // --- send ---
    svm.send_transaction(tx).expect("pool instruction failed");

    // --- verify pool state ---
    let pool_account = svm
        .get_account(&pool_pda)
        .expect("pool PDA should exist after init");

    let mut data: &[u8] = &pool_account.data;
    let pool_state =
        PoolState::try_deserialize(&mut data).expect("failed to deserialize PoolState");

    let expected_authority_bytes = payer_addr.as_ref();
    assert_eq!(
        pool_state.authority.to_bytes(),
        expected_authority_bytes,
        "authority mismatch"
    );

    assert_eq!(pool_state.next_leaf_index, 0);
    assert_eq!(pool_state.total_deposits, 0);
    assert_eq!(pool_state.current_root_index, 0);
    assert_eq!(pool_state.roots, [[0u8; 32]; 10]);

    // --- verify vault exists (empty) ---
    let vault_account = svm
        .get_account(&vault_pda)
        .expect("vault PDA should exist after init");
    assert_eq!(vault_account.data.len(), 0, "vault must have no data");
}
