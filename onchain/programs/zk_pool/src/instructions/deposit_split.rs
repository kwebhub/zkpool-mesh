//! Instruction: `deposit_split` — deposit SOL and create N commitments in one call.
//!
//! Stage 15: the depositor splits one deposit into `SPLIT_COUNT` (= 3) unequal
//! parts, each producing its own commitment. This breaks the on-chain amount
//! correlation between deposit and withdrawal.
//!
//! The depositor provides:
//!   - `commitments[i]`:  32-byte hash of (nullifier_i, secret_i, amount_i).
//!   - `new_roots[i]`:    Merkle root after inserting commitments[0..=i].
//!                        new_roots[0] = root after inserting commitment[0],
//!                        new_roots[1] = root after inserting commitment[0..2],
//!                        new_roots[2] = root after inserting all three (final).
//!   - `amounts[i]`:      per-commitment amount in lamports.
//!   - `total_amount`:    Σ amounts[i]. Must equal the sum on-chain.
//!
//! ## Trust model — IMPORTANT
//!
//! Like `deposit`, this instruction does NOT verify on-chain that
//! `new_roots[i]` are correct Merkle roots. A malicious depositor could
//! submit garbage roots and corrupt the tree. Known limitation, see
//! `docs/DEMO-NOTICE.md`.
//!
//! ## Flow
//!
//!   1. Validate each `amounts[i] >= MIN_DEPOSIT_AMOUNT`.
//!   2. Validate the tree has room for `SPLIT_COUNT` leaves.
//!   3. Validate `Σ amounts[i] == total_amount` (overflow-checked).
//!   4. Validate `new_roots[SPLIT_COUNT-1] != current_root`.
//!   5. Transfer `total_amount` SOL from depositor to vault (one CPI).
//!   6. For each i in 0..SPLIT_COUNT:
//!        - emit `DepositEvent { commitment_i, leaf_index_i, new_root_i, ts }`,
//!        - advance `next_leaf_index` by 1 (checked).
//!   7. Add **only the final root** to the roots ring buffer.
//!   8. Increment `total_deposits` by 1 (one deposit transaction).
//!
//! ## Why one `add_root` and not three
//!
//! The three commitments are inserted atomically — there is no window
//! where commitment[0] exists in the tree but commitments[1..3] do not.
//! The final root contains all three leaves; any of the three notes can
//! be withdrawn against it. Storing the intermediate roots would consume
//! ring-buffer slots with no benefit.

use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

use crate::constants::{MAX_LEAVES, MIN_DEPOSIT_AMOUNT, POOL_SEED, SPLIT_COUNT, VAULT_SEED};
use crate::error::ZkPoolError;
use crate::events::DepositEvent;
use crate::state::PoolState;

/// Arguments for `deposit_split`.
///
/// Grouped into a struct so the Anchor IDL and the Codama-generated client
/// present a single named argument with a stable shape, rather than four
/// flat parameters of array types.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct DepositSplitArgs {
    /// Commitments, one per split. `commitments[i] = hash_3(nullifier_i, secret_i, amount_i)`.
    pub commitments: [[u8; 32]; SPLIT_COUNT],

    /// Merkle roots, one per split.
    /// `new_roots[i]` is the root after inserting `commitments[0..=i]`.
    pub new_roots: [[u8; 32]; SPLIT_COUNT],

    /// Per-split amounts, in lamports.
    pub amounts: [u64; SPLIT_COUNT],

    /// Aggregate amount = Σ `amounts[i]`. Transferred to the vault in one CPI.
    pub total_amount: u64,
}

#[derive(Accounts)]
pub struct DepositSplit<'info> {
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

/// Handler for the `deposit_split` instruction.
pub fn handler_deposit_split(ctx: Context<DepositSplit>, args: DepositSplitArgs) -> Result<()> {
    let pool = &mut ctx.accounts.pool;

    // ----- 1. Validate each amount -----
    for amount in args.amounts.iter() {
        require!(
            *amount >= MIN_DEPOSIT_AMOUNT,
            ZkPoolError::DepositBelowMinimum
        );
    }

    // ----- 2. Validate tree has room for SPLIT_COUNT leaves -----
    let leaves_needed = SPLIT_COUNT as u64;
    require!(
        pool.next_leaf_index + leaves_needed <= MAX_LEAVES,
        ZkPoolError::NotEnoughRoom
    );

    // ----- 3. Validate Σ amounts[i] == total_amount (overflow-checked) -----
    let mut sum: u64 = 0;
    for amount in args.amounts.iter() {
        sum = sum
            .checked_add(*amount)
            .ok_or(ZkPoolError::SplitSumMismatch)?;
    }
    require!(sum == args.total_amount, ZkPoolError::SplitSumMismatch);

    // ----- 4. Validate the final root differs from current root -----
    let final_root = args.new_roots[SPLIT_COUNT - 1];
    let current_root = pool.current_root();
    require!(final_root != current_root, ZkPoolError::RootUnchanged);

    // ----- 5. Transfer total_amount from depositor to vault (one CPI) -----
    let cpi_accounts = Transfer {
        from: ctx.accounts.depositor.to_account_info(),
        to: ctx.accounts.vault.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.system_program.key(), cpi_accounts);
    transfer(cpi_ctx, args.total_amount)?;

    // ----- 6. Emit one event per commitment, advance next_leaf_index per commitment -----
    let timestamp = Clock::get()?.unix_timestamp;
    for i in 0..SPLIT_COUNT {
        let leaf_index = pool.next_leaf_index;
        pool.next_leaf_index = leaf_index.checked_add(1).ok_or(ZkPoolError::TreeFull)?;

        emit!(DepositEvent {
            commitment: args.commitments[i],
            leaf_index,
            new_root: args.new_roots[i],
            timestamp,
        });

        msg!(
            "DepositSplit[{}/{}]: commitment_first_byte={}, leaf_index={}, amount={}, new_root_first_byte={}",
            i + 1,
            SPLIT_COUNT,
            args.commitments[i][0],
            leaf_index,
            args.amounts[i],
            args.new_roots[i][0],
        );
    }

    // ----- 7. Add only the final root to the ring buffer -----
    pool.add_root(final_root);

    // ----- 8. Increment total_deposits by 1 (one deposit transaction) -----
    pool.total_deposits = pool
        .total_deposits
        .checked_add(1)
        .ok_or(ZkPoolError::TreeFull)?;

    msg!(
        "DepositSplit: total_amount={}, splits={}, final_root_first_byte={}",
        args.total_amount,
        SPLIT_COUNT,
        final_root[0]
    );

    Ok(())
}
