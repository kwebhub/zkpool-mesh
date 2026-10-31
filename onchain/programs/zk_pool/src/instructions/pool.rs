//! Instruction: `pool` — initialize the pool.
//!
//! Creates two PDAs:
//!   1. `PoolState` — pool metadata (authority, roots ring buffer, counters).
//!   2. `vault`     — SOL vault, owned by the pool PDA.
//!
//! Can be called only once. The `init` constraint on `PoolState` will fail
//! if the account already exists.

use anchor_lang::prelude::*;

use crate::constants::{POOL_SEED, ROOT_HISTORY_SIZE, VAULT_SEED};
use crate::state::PoolState;

#[derive(Accounts)]
pub struct Pool<'info> {
    /// Pool authority (creator). Pays for account creation.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Pool state PDA.
    ///
    /// Seeds: `[POOL_SEED]`.
    /// Fails if the account already exists — pool can be initialized only once.
    #[account(
        init,
        payer = authority,
        space = 8 + PoolState::INIT_SPACE,
        seeds = [POOL_SEED],
        bump,
    )]
    pub pool: Account<'info, PoolState>,

    /// SOL vault PDA.
    ///
    /// Seeds: `[VAULT_SEED, pool]`.
    /// Not initialized with data — used only to hold lamports.
    #[account(
        init,
        payer = authority,
        space = 0,
        seeds = [VAULT_SEED, pool.key().as_ref()],
        bump,
    )]
    /// CHECK: this account holds SOL only; it has no data and is never deserialized.
    pub vault: UncheckedAccount<'info>,

    /// Solana system program (required for `init`).
    pub system_program: Program<'info, System>,
}

/// Handler for the `pool` instruction.
pub fn handler_pool(ctx: Context<Pool>) -> Result<()> {
    let pool = &mut ctx.accounts.pool;

    pool.authority = ctx.accounts.authority.key();
    pool.next_leaf_index = 0;
    pool.total_deposits = 0;
    pool.current_root_index = 0;
    pool.roots = [[0u8; 32]; ROOT_HISTORY_SIZE];

    msg!(
        "Pool initialized: authority={}, vault={}",
        pool.authority,
        ctx.accounts.vault.key()
    );

    Ok(())
}
