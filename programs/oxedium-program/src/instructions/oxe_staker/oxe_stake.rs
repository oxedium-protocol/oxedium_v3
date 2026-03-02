use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Mint as TokenMint,
    token_2022::{self, Token2022, TransferChecked},
    token_interface::{Mint, TokenAccount},
};

use crate::{
    components::{calculate_staker_yield, snapshot_oxe_checkpoints},
    events::OxeStakeEvent,
    states::{OxeCheckpoint, OxeGlobalState, OxeStaker, Vault},
    utils::{OXE_CHECKPOINT_SEED, OXE_GLOBAL_SEED, OXE_STAKER_SEED, OxediumError, VAULT_SEED},
};

/// Stake OXE (Token22) tokens.
///
/// Creates the [`OxeStaker`] position and the [`OxeCheckpoint`] for `vault_pda`
/// on the first call (via `init_if_needed`). On subsequent calls the named
/// checkpoint is snapshotted at the current balance before it increases.
///
/// # remaining_accounts
/// Pairs of `[vault_pda (readonly), oxe_checkpoint_pda (mut), ...]` for all
/// *other* vaults in which the staker already has an existing OxeCheckpoint.
/// These are snapshotted at the old balance before the balance increases.
pub fn oxe_stake(ctx: Context<OxeStakeInstructionAccounts>, amount: u64) -> Result<()> {
    let old_balance = ctx.accounts.oxe_staker_pda.staked_amount;

    // Handle named vault checkpoint: init (no retroactive yield) or snapshot
    {
        let checkpoint = &mut ctx.accounts.oxe_checkpoint_pda;
        let vault = &ctx.accounts.vault_pda;

        if checkpoint.owner == Pubkey::default() {
            // Newly created: pin to current accumulator so yield accrues from now
            checkpoint.owner = ctx.accounts.signer.key();
            checkpoint.vault = vault.key();
            checkpoint.last_oxe_cumulative_yield = vault.oxe_cumulative_yield_per_staker;
            checkpoint.pending_yield = 0;
        } else {
            // Already exists: accrue yield earned at old balance before it changes
            let earned = calculate_staker_yield(
                vault.oxe_cumulative_yield_per_staker,
                old_balance,
                checkpoint.last_oxe_cumulative_yield,
            )?;
            checkpoint.pending_yield = checkpoint
                .pending_yield
                .checked_add(earned)
                .ok_or(OxediumError::OverflowInAdd)?;
            checkpoint.last_oxe_cumulative_yield = vault.oxe_cumulative_yield_per_staker;
        }
    }

    // Snapshot all other existing per-vault checkpoints at old balance
    snapshot_oxe_checkpoints(
        ctx.remaining_accounts,
        old_balance,
        ctx.accounts.signer.key(),
    )?;

    // Transfer OXE (Token22) from signer to global escrow ATA
    let cpi_accounts = TransferChecked {
        from: ctx.accounts.signer_oxe_ata.to_account_info(),
        mint: ctx.accounts.oxe_mint.to_account_info(),
        to: ctx.accounts.oxe_vault_ata.to_account_info(),
        authority: ctx.accounts.signer.to_account_info(),
    };
    token_2022::transfer_checked(
        CpiContext::new(ctx.accounts.token_program_2022.to_account_info(), cpi_accounts),
        amount,
        ctx.accounts.oxe_mint.decimals,
    )?;

    // Update global total
    ctx.accounts.oxe_global_state.total_staked = ctx
        .accounts
        .oxe_global_state
        .total_staked
        .checked_add(amount)
        .ok_or(OxediumError::OverflowInAdd)?;

    // Update staker position (set owner on first stake)
    if ctx.accounts.oxe_staker_pda.owner == Pubkey::default() {
        ctx.accounts.oxe_staker_pda.owner = ctx.accounts.signer.key();
    }
    ctx.accounts.oxe_staker_pda.staked_amount = ctx
        .accounts
        .oxe_staker_pda
        .staked_amount
        .checked_add(amount)
        .ok_or(OxediumError::OverflowInAdd)?;

    emit!(OxeStakeEvent {
        user: ctx.accounts.signer.key(),
        amount,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct OxeStakeInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    /// SPL mint of the vault token — used to derive vault_pda.
    pub vault_token_mint: Account<'info, TokenMint>,

    #[account(
        seeds = [VAULT_SEED.as_bytes(), vault_token_mint.key().as_ref()],
        bump,
    )]
    pub vault_pda: Account<'info, Vault>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [OXE_CHECKPOINT_SEED.as_bytes(), vault_pda.key().as_ref(), signer.key().as_ref()],
        bump,
        space = 8 + 32 + 32 + 16 + 8,
    )]
    pub oxe_checkpoint_pda: Account<'info, OxeCheckpoint>,

    #[account(
        constraint = oxe_mint.key() == oxe_global_state.oxe_mint @ OxediumError::InvalidMint
    )]
    pub oxe_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        token::mint = oxe_mint,
        token::authority = signer,
        token::token_program = token_program_2022,
    )]
    pub signer_oxe_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [OXE_GLOBAL_SEED.as_bytes()],
        bump,
    )]
    pub oxe_global_state: Account<'info, OxeGlobalState>,

    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = oxe_mint,
        associated_token::authority = oxe_global_state,
        associated_token::token_program = token_program_2022,
    )]
    pub oxe_vault_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [OXE_STAKER_SEED.as_bytes(), signer.key().as_ref()],
        bump,
        space = 8 + 32 + 8,
    )]
    pub oxe_staker_pda: Account<'info, OxeStaker>,

    pub token_program_2022: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
