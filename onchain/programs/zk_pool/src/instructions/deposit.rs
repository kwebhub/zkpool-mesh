//! Instruction: `deposit` — deposit SOL into the pool.
//!
//! The depositor provides:
//!   - `commitment`: a 32-byte hash of (nullifier, secret, amount).
//!   - `new_root`:   the new Merkle root after inserting the commitment.
//!   - `amount`:     the deposit amount in lamports.
//!
//! ## Trust model — IMPORTANT
//!
//! This instruction DOES NOT verify on-chain that `new_root` is the correct
//! result of inserting `commitment` into the current tree. A malicious
//! depositor could submit a garbage `new_root` and corrupt the tree for
//! everyone.
//!
//! This is a known limitation, inherited from v2. Fixing it requires either
//! maintaining the full Merkle tree on-chain or verifying an additional
//! ZK-proof of tree update correctness — both are out of scope for v3.
//!
//! For production use, see `docs/DEMO-NOTICE.md`.
//!
//! ## Flow
//!
//!   1. Validate `amount >= MIN_DEPOSIT_AMOUNT`.
//!   2. Validate the tree has room.
//!   3. Validate `new_root != current_root`.
//!   4. Transfer SOL from depositor to vault (CPI to System Program).
//!   5. Update `PoolState` (leaf index, deposit counter, roots ring buffer).
//!   6. Emit `DepositEvent`.

use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::constants::{MIN_DEPOSIT_AMOUNT, POOL_SEED, VAULT_SEED};
use crate::error::ZkPoolError;
use crate::events::DepositEvent;
use crate::state::PoolState;

#[derive(Accounts)]
pub struct Deposit<'info> {
    /// Depositor. Pays for the deposit and signs the transaction.
    #[account(mut)]
    pub depositor: Signer<'info>,

    /// Pool state PDA. Seeds: `[POOL_SEED]`.
    #[account(
        mut,
        seeds = [POOL_SEED],
        bump,
    )]
    pub pool: Account<'info, PoolState>,

    /// SOL vault PDA. Seeds: `[VAULT_SEED, pool]`.
    #[account(
        mut,
        seeds = [VAULT_SEED, pool.key().as_ref()],
        bump,
    )]
    /// CHECK: this account holds SOL only; it has no data and is never deserialized.
    pub vault: UncheckedAccount<'info>,

    /// Solana system program (required for CPI transfer).
    pub system_program: Program<'info, System>,
}

/// Handler for the `deposit` instruction.
pub fn handler_deposit(
    ctx: Context<Deposit>,
    commitment: [u8; 32],
    new_root: [u8; 32],
    amount: u64,
) -> Result<()> {
    let pool = &mut ctx.accounts.pool;

    // ----- 1. Validate amount -----
    require!(
        amount >= MIN_DEPOSIT_AMOUNT,
        ZkPoolError::DepositBelowMinimum
    );

    // ----- 2. Validate tree has room -----
    require!(pool.has_room(), ZkPoolError::TreeFull);

    // ----- 3. Validate new_root differs from current root -----
    let current_root = pool.current_root();
    require!(new_root != current_root, ZkPoolError::RootUnchanged);

    // ----- 4. Transfer SOL from depositor to vault -----
    //
    // Anchor 1.2.0: CpiContext::new takes the program Pubkey, not AccountInfo.
    let cpi_accounts = Transfer {
        from: ctx.accounts.depositor.to_account_info(),
        to: ctx.accounts.vault.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.system_program.key(), cpi_accounts);
    transfer(cpi_ctx, amount)?;

    // ----- 5. Update pool state -----
    let leaf_index = pool.next_leaf_index;
    pool.next_leaf_index = leaf_index.checked_add(1).ok_or(ZkPoolError::TreeFull)?;
    pool.total_deposits = pool.total_deposits.checked_add(1).unwrap();
    pool.add_root(new_root);

    // ----- 6. Emit event -----
    emit!(DepositEvent {
        commitment,
        leaf_index,
        new_root,
        timestamp: Clock::get()?.unix_timestamp,
    });

    msg!(
        "Deposit: commitment_first_byte={}, leaf_index={}, amount={}",
        commitment[0],
        leaf_index,
        amount
    );

    Ok(())
}
