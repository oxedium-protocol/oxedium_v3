use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_2022::{self, Token2022, TransferChecked},
    token_interface::{Mint, TokenAccount},
};

use crate::{
    components::snapshot_oxe_checkpoints,
    events::OxeUnstakeEvent,
    states::{OxeGlobalState, OxeStaker},
    utils::{OXE_GLOBAL_SEED, OXE_STAKER_SEED, OxediumError},
};

/// Unstake OXE (Token22) tokens instantly (no cooldown).
///
/// # remaining_accounts
/// Pairs of `[vault_pda (readonly), oxe_checkpoint_pda (mut), ...]` for all
/// vaults in which the staker has an existing OxeCheckpoint.
/// These MUST be passed to avoid losing pending yield when balance decreases.
pub fn oxe_unstake(ctx: Context<OxeUnstakeInstructionAccounts>, amount: u64) -> Result<()> {
    require!(amount > 0, OxediumError::ZeroAmount);

    let old_balance = ctx.accounts.oxe_staker_pda.staked_amount;
    require!(old_balance >= amount, OxediumError::InsufficientBalance);

    // Snapshot all existing per-vault checkpoints at the current balance
    snapshot_oxe_checkpoints(
        ctx.remaining_accounts,
        old_balance,
        ctx.accounts.signer.key(),
    )?;

    // PDA-signed transfer from escrow ATA back to signer
    let global_bump = ctx.bumps.oxe_global_state;
    let seeds: &[&[u8]] = &[OXE_GLOBAL_SEED.as_bytes(), &[global_bump]];
    let signer_seeds: &[&[&[u8]]] = &[&seeds[..]];

    let cpi_accounts = TransferChecked {
        from: ctx.accounts.oxe_vault_ata.to_account_info(),
        mint: ctx.accounts.oxe_mint.to_account_info(),
        to: ctx.accounts.signer_oxe_ata.to_account_info(),
        authority: ctx.accounts.oxe_global_state.to_account_info(),
    };
    token_2022::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program_2022.to_account_info(),
            cpi_accounts,
            signer_seeds,
        ),
        amount,
        ctx.accounts.oxe_mint.decimals,
    )?;

    // Update global state and staker position
    ctx.accounts.oxe_global_state.total_staked = ctx
        .accounts
        .oxe_global_state
        .total_staked
        .checked_sub(amount)
        .ok_or(OxediumError::OverflowInSub)?;

    ctx.accounts.oxe_staker_pda.staked_amount = ctx
        .accounts
        .oxe_staker_pda
        .staked_amount
        .checked_sub(amount)
        .ok_or(OxediumError::OverflowInSub)?;

    emit!(OxeUnstakeEvent {
        user: ctx.accounts.signer.key(),
        amount,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct OxeUnstakeInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        constraint = oxe_mint.key() == oxe_global_state.oxe_mint @ OxediumError::InvalidMint
    )]
    pub oxe_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = oxe_mint,
        associated_token::authority = signer,
        associated_token::token_program = token_program_2022,
    )]
    pub signer_oxe_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [OXE_GLOBAL_SEED.as_bytes()],
        bump,
    )]
    pub oxe_global_state: Account<'info, OxeGlobalState>,

    #[account(
        mut,
        associated_token::mint = oxe_mint,
        associated_token::authority = oxe_global_state,
        associated_token::token_program = token_program_2022,
    )]
    pub oxe_vault_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [OXE_STAKER_SEED.as_bytes(), signer.key().as_ref()],
        bump,
        constraint = oxe_staker_pda.owner == signer.key() @ OxediumError::InvalidStaker,
    )]
    pub oxe_staker_pda: Account<'info, OxeStaker>,

    pub token_program_2022: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
