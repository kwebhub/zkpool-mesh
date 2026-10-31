//! Integration tests for the `deposit` instruction.
//!
//! Stage 4.5.4 — minimal test: init pool, then deposit, then verify
//! vault balance, next_leaf_index, total_deposits.

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
const DEPOSIT_DISCRIMINATOR: [u8; 8] = [242, 35, 198, 137, 82, 225, 242, 182];

fn system_program_id() -> Address {
    "11111111111111111111111111111111".parse().unwrap()
}

fn find_pda(seeds: &[&[u8]], program_id: &Address) -> (Address, u8) {
    Address::find_program_address(seeds, program_id)
}

/// Helper: initialize the pool and return (payer, pool_pda, vault_pda).
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

#[test]
fn test_deposit_updates_state_and_vault() {
    let mut svm: LiteSVM = setup_svm();
    let (payer, pool_pda, vault_pda) = init_pool(&mut svm);

    let payer_addr: Address = payer.pubkey().into();

    // Vault has a rent-exempt minimum balance right after init, even though
    // it holds no user funds. Remember it and check the delta later.
    //
    // In Solana, an empty (0-byte data) account must hold at least
    // `rent-exempt minimum` lamports to exist. LiteSVM/Anchor create the
    // vault PDA with exactly this minimum. This is NOT user funds.
    let vault_initial = svm.get_balance(&vault_pda).expect("vault exists");
    // Sanity: it should be 890880 for an empty account on devnet defaults.
    // We don't hard-assert the exact number, only that it is present.
    assert!(vault_initial > 0, "vault must be rent-exempt after init");

    // Build deposit instruction.
    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let commitment = [0x01u8; 32];
    let new_root = [0x02u8; 32];
    let amount: u64 = 1_000_000; // 0.001 SOL (MIN_DEPOSIT_AMOUNT)

    // Instruction data: discriminator + commitment + new_root + amount (LE u64).
    let mut data = Vec::with_capacity(8 + 32 + 32 + 8);
    data.extend_from_slice(&DEPOSIT_DISCRIMINATOR);
    data.extend_from_slice(&commitment);
    data.extend_from_slice(&new_root);
    data.extend_from_slice(&amount.to_le_bytes());

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer_addr, true),                    // depositor
            AccountMeta::new(pool_pda, false),                     // pool
            AccountMeta::new(vault_pda, false),                    // vault
            AccountMeta::new_readonly(system_program_id(), false), // system_program
        ],
        data,
    };

    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash);

    svm.send_transaction(tx).expect("deposit failed");

    // Vault balance increased by exactly `amount`.
    let vault_after = svm.get_balance(&vault_pda).expect("vault exists");
    assert_eq!(
        vault_after,
        vault_initial + amount,
        "vault balance must increase by deposit amount"
    );

    // Pool state updated.
    let pool_account = svm.get_account(&pool_pda).expect("pool PDA exists");
    let mut data: &[u8] = &pool_account.data;
    let pool_state = PoolState::try_deserialize(&mut data).expect("deserialize PoolState");

    assert_eq!(pool_state.next_leaf_index, 1);
    assert_eq!(pool_state.total_deposits, 1);
    assert!(pool_state.is_known_root(&new_root));
}
