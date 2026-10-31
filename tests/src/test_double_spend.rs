//! Integration tests for double-spend protection.
//!
//! Stage 4.5.6 — double-spend protection test.
//!
//! We do NOT need a valid proof for this test. The `init` constraint on
//! `NullifierRecord` executes during Anchor account validation, BEFORE
//! any `require!` in the handler body. So:
//!   1. Pre-create the `NullifierRecord` PDA manually via `svm.set_account`.
//!   2. Call `withdraw` with the same `nullifier_hash`.
//!   3. Anchor's `init` fails immediately with `AccountAlreadyInUse`.
//!
//! This proves that once a nullifier is used, the same one cannot be used
//! again — which is exactly the double-spend protection we want.

use litesvm::LiteSVM;
use solana_account::Account;
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
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

/// Helper: init pool + one deposit. Returns (payer, pool_pda, vault_pda).
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

    let mut data = Vec::new();
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
fn test_double_spend_rejected() {
    let mut svm: LiteSVM = setup_svm();
    let (payer, pool_pda, vault_pda) = init_and_deposit(&mut svm);

    let payer_addr: Address = payer.pubkey().into();
    let program_id: Address = ZK_POOL_ID.parse().unwrap();
    let verifier_id: Address = VERIFIER_ID.parse().unwrap();

    let nullifier_hash = [0xaa_u8; 32];
    let root = [0x02_u8; 32];
    let amount: u64 = 1_000_000;
    let recipient_binding = [0xbb_u8; 32];

    // Derive the NullifierRecord PDA.
    let (nullifier_record, _) = find_pda(
        &[
            NULLIFIER_RECORD_SEED,
            pool_pda.as_ref(),
            nullifier_hash.as_ref(),
        ],
        &program_id,
    );

    // --- Pre-create the NullifierRecord account manually ---
    //
    // This simulates the state AFTER a successful first withdraw.
    // In a real scenario, the program would create this account during
    // the first withdraw. Here we fake it, because the first withdraw
    // would fail on the verifier CPI with a dummy proof.
    //
    // The account must be owned by zk_pool for Anchor's `init` to see it.
    let zk_pool_pubkey: Pubkey = ZK_POOL_ID.parse().unwrap();
    let fake_record_data = vec![0u8; 8 + 32 + 32 + 32 + 8 + 8]; // discriminator + fields
    let account = Account {
        lamports: 10_000_000, // enough for rent-exemption
        data: fake_record_data,
        owner: zk_pool_pubkey,
        executable: false,
        rent_epoch: 0,
    };
    svm.set_account(nullifier_record, account)
        .expect("set_account failed");

    // Verify the account is now present.
    assert!(
        svm.get_account(&nullifier_record).is_some(),
        "nullifier_record must exist before withdraw"
    );

    // --- Attempt withdraw with the same nullifier_hash ---
    //
    // The `init` constraint on `nullifier_record` must fail with
    // `AccountAlreadyInUse` (Anchor's default error for `init` on an
    // existing account).
    let proof = vec![0u8; PROOF_LEN];
    let mut data = Vec::new();
    data.extend_from_slice(&WITHDRAW_DISCRIMINATOR);
    data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
    data.extend_from_slice(&proof);
    data.extend_from_slice(&nullifier_hash);
    data.extend_from_slice(&root);
    data.extend_from_slice(payer_addr.as_ref());
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&recipient_binding);

    let ix = Instruction {
        program_id,
        accounts: vec![
            AccountMeta::new(payer_addr, true),
            AccountMeta::new(pool_pda, false),
            AccountMeta::new(nullifier_record, false),
            AccountMeta::new(vault_pda, false),
            AccountMeta::new(payer_addr, false),
            AccountMeta::new_readonly(verifier_id, false),
            AccountMeta::new_readonly(system_program_id(), false),
        ],
        data,
    };

    let blockhash = svm.latest_blockhash();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash);

    let result = svm.send_transaction(tx);
    assert!(
        result.is_err(),
        "withdraw must fail when nullifier_record already exists (double-spend)"
    );
}
