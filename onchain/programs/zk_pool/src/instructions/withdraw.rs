//! Instruction: `withdraw` — withdraw SOL from the pool.
//!
//! The caller provides:
//!   - `proof`:             388-byte Groth16 proof (Sunspot 1.0.0, Stage 15.4).
//!   - `nullifier_hash`:    public input, prevents double-spend.
//!   - `root`:              public input, must be in the roots history.
//!   - `recipient`:         public input, must equal the `to` account.
//!   - `amount`:            public input, this note's amount (one of the splits).
//!   - `total_amount`:      public input, aggregate deposit amount (Stage 15).
//!   - `recipient_binding`: public input, ties the proof to `recipient`.
//!
//! ## Flow
//!
//!   1. Validate proof length.
//!   2. Validate `recipient` matches the `to` account.
//!   3. Validate `root` is in the pool's roots history.
//!   4. Encode the 6 public inputs into a 204-byte blob.
//!   5. Invoke the verifier program via CPI with `[proof || public_witness]`.
//!   6. Create the `NullifierRecord` PDA — this fails if it already exists,
//!      which is how double-spend is prevented.
//!   7. Transfer `amount` lamports from the vault to `recipient`.
//!   8. Emit `WithdrawEvent`.
//!
//! ## The critical part
//!
//! Step 4 uses `encode_public_inputs`, which MUST produce the exact same
//! 204 bytes that:
//!   - `sunspot prove` writes to `withdrawal.pw` (stage 15.4),
//!   - the frontend produces (stage 15.8).
//!
//! This is where v2 broke (`InvalidInstructionData`). The layout is defined
//! in `circuits/withdrawal/spec.json`; any change requires updating all
//! layers and re-running `validate-spec`.
//!
//! ## CPI data layout (from verifier-bin source)
//!
//! The Sunspot verifier expects:
//!
//! ```text
//! [proof: PROOF_LEN bytes][public_witness: PUBLIC_INPUTS_BYTES bytes]
//! ```
//!
//! i.e. PROOF FIRST, then PUBLIC WITNESS. The verifier computes:
//! ```rust
//! let proof_len = instruction_data.len() - (12 + NR_INPUTS * 32);
//! let proof_bytes = &instruction_data[..proof_len];
//! let public_witness_bytes = &instruction_data[proof_len..];
//! ```
//!
//! `12 + NR_INPUTS * 32 = 12 + 6 * 32 = 204` matches `PUBLIC_INPUTS_BYTES`.

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::invoke_signed;

use crate::constants::{
    NULLIFIER_RECORD_SEED, POOL_SEED, PROOF_LEN, PUBLIC_INPUTS_BYTES, VAULT_SEED,
    VERIFIER_PROGRAM_ID,
};
use crate::encoding::encode_public_inputs;
use crate::error::ZkPoolError;
use crate::events::WithdrawEvent;
use crate::state::{NullifierRecord, PoolState};

#[derive(Accounts)]
#[instruction(proof: Vec<u8>, nullifier_hash: [u8; 32])]
pub struct Withdraw<'info> {
    /// Caller. Pays for the NullifierRecord PDA creation.
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Pool state PDA. Seeds: `[POOL_SEED]`.
    #[account(
        mut,
        seeds = [POOL_SEED],
        bump,
    )]
    pub pool: Account<'info, PoolState>,

    /// Nullifier record PDA.
    ///
    /// Seeds: `[NULLIFIER_RECORD_SEED, pool, nullifier_hash]`.
    /// `init` fails if the account already exists — double-spend protection.
    #[account(
        init,
        payer = payer,
        space = 8 + NullifierRecord::INIT_SPACE,
        seeds = [NULLIFIER_RECORD_SEED, pool.key().as_ref(), nullifier_hash.as_ref()],
        bump,
    )]
    pub nullifier_record: Account<'info, NullifierRecord>,

    /// SOL vault PDA. Seeds: `[VAULT_SEED, pool]`.
    #[account(
        mut,
        seeds = [VAULT_SEED, pool.key().as_ref()],
        bump,
    )]
    /// CHECK: this account holds SOL only; it has no data and is never deserialized.
    pub vault: UncheckedAccount<'info>,

    /// Recipient of the withdrawn SOL.
    /// Must match the `recipient` public input.
    #[account(mut)]
    /// CHECK: recipient can be any account; we only send lamports to it.
    pub to: UncheckedAccount<'info>,

    /// Verifier program (deployed in stage 3.4, upgraded in place in stage 15.4).
    /// CHECK: address is validated against VERIFIER_PROGRAM_ID.
    #[account(address = VERIFIER_PROGRAM_ID)]
    pub verifier_program: UncheckedAccount<'info>,

    /// Solana system program.
    pub system_program: Program<'info, System>,
}

#[allow(clippy::too_many_arguments)]
pub fn handler_withdraw(
    ctx: Context<Withdraw>,
    proof: Vec<u8>,
    nullifier_hash: [u8; 32],
    root: [u8; 32],
    recipient: Pubkey,
    amount: u64,
    recipient_binding: [u8; 32],
    total_amount: u64,
) -> Result<()> {
    // ----- 1. Validate proof length -----
    require!(proof.len() == PROOF_LEN, ZkPoolError::InvalidProofLength);

    // ----- 2. Validate recipient matches `to` -----
    require_keys_eq!(
        recipient,
        ctx.accounts.to.key(),
        ZkPoolError::RecipientMismatch
    );

    // ----- 3. Validate root is known -----
    require!(
        ctx.accounts.pool.is_known_root(&root),
        ZkPoolError::UnknownRoot
    );

    // ----- 4. Encode public inputs (204 bytes) -----
    let public_inputs = encode_public_inputs(
        &root,
        &nullifier_hash,
        &recipient,
        &recipient_binding,
        amount,
        total_amount,
    );
    debug_assert_eq!(public_inputs.len(), PUBLIC_INPUTS_BYTES);

    // ----- 5. Invoke verifier via CPI -----
    //
    // Layout expected by verifier-bin:
    //   [proof: PROOF_LEN bytes][public_witness: PUBLIC_INPUTS_BYTES bytes]
    //
    // The verifier computes proof_len = total - 204, so PROOF MUST COME FIRST.
    let mut data = Vec::with_capacity(PROOF_LEN + PUBLIC_INPUTS_BYTES);
    data.extend_from_slice(&proof);
    data.extend_from_slice(&public_inputs);

    let ix = Instruction {
        program_id: VERIFIER_PROGRAM_ID,
        accounts: vec![],
        data,
    };

    invoke_signed(&ix, &[ctx.accounts.verifier_program.to_account_info()], &[])?;

    // ----- 6. Create NullifierRecord (double-spend protection) -----
    let record = &mut ctx.accounts.nullifier_record;
    record.pool = ctx.accounts.pool.key();
    record.nullifier_hash = nullifier_hash;
    record.recipient = recipient;
    record.amount = amount;
    record.timestamp = Clock::get()?.unix_timestamp;

    // ----- 7. Transfer SOL from vault to recipient -----
    //
    // The vault is a PDA of this program with no private key. We use
    // direct lamport manipulation via `try_borrow_mut_lamports`. This is
    // safe because the vault PDA seeds are validated above.
    let vault_lamports = ctx.accounts.vault.lamports();
    require!(
        vault_lamports >= amount,
        ZkPoolError::InsufficientVaultBalance
    );

    **ctx.accounts.vault.try_borrow_mut_lamports()? = vault_lamports
        .checked_sub(amount)
        .ok_or(ZkPoolError::InsufficientVaultBalance)?;

    **ctx.accounts.to.try_borrow_mut_lamports()? = ctx
        .accounts
        .to
        .lamports()
        .checked_add(amount)
        .ok_or(ZkPoolError::InsufficientVaultBalance)?;

    // ----- 8. Emit event -----
    emit!(WithdrawEvent {
        nullifier_hash,
        recipient,
        amount,
        timestamp: Clock::get()?.unix_timestamp,
    });

    msg!(
        "Withdraw: nullifier_hash_first_byte={}, recipient={}, amount={}, total_amount={}",
        nullifier_hash[0],
        recipient,
        amount,
        total_amount
    );

    Ok(())
}
