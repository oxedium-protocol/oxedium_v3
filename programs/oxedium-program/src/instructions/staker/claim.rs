use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::{components::calculate_staker_yield, events::ClaimEvent, states::{Staker, Vault}, utils::{VAULT_SEED, STAKER_SEED, OxediumError}};

/// Claim accumulated yield for a staker from a vault
///
/// # Arguments
/// * `ctx` - context containing all accounts required for claiming
pub fn claim(ctx: Context<ClaimInstructionAccounts>) -> Result<()> {
    let vault_pda_info = ctx.accounts.vault_pda.to_account_info();

    let vault: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda;
    let staker: &mut Account<'_, Staker> = &mut ctx.accounts.staker_pda;

    let cumulative_yield_per_lp: u128 = vault.cumulative_yield_per_lp;
    let staker_balance: u64 = staker.staked_amount;
    let staker_last_cumulative_yield: u128 = staker.last_cumulative_yield;
    let staker_pending_claim: u64 = staker.pending_claim;

    let staker_yield: u64 = calculate_staker_yield(cumulative_yield_per_lp, staker_balance, staker_last_cumulative_yield);
    let amount: u64 = staker_yield
        .checked_add(staker_pending_claim)
        .ok_or(OxediumError::OverflowInAdd)?;

    require!(amount > 0, OxediumError::ZeroAmount);

    let mint_key = ctx.accounts.vault_mint.key();
    let seeds = &[VAULT_SEED.as_bytes(), mint_key.as_ref(), &[ctx.bumps.vault_pda]];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.vault_ata.to_account_info(),
        to: ctx.accounts.signer_ata.to_account_info(),
        authority: vault_pda_info
    };

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds),
        amount)?;

    vault.current_balance = vault.current_balance
        .checked_sub(amount)
        .ok_or(OxediumError::OverflowInSub)?;

    staker.last_cumulative_yield = cumulative_yield_per_lp;
    staker.pending_claim = 0;

    emit!(ClaimEvent {
        user: ctx.accounts.signer.key(),
        mint: vault.token_mint.key(),
        amount: amount
    });

    Ok(())
}

/// Accounts context for the claim instruction
#[derive(Accounts)]
pub struct ClaimInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub vault_mint: Account<'info, Mint>,

    #[account(mut, token::authority = signer, token::mint = vault_mint)]
    pub signer_ata: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [STAKER_SEED.as_bytes(), vault_pda.key().as_ref(), signer.key().as_ref()],
        bump,
        constraint = staker_pda.owner == signer.key() @ OxediumError::InvalidStaker,
        constraint = staker_pda.vault == vault_pda.key() @ OxediumError::InvalidVault,
    )]
    pub staker_pda: Account<'info, Staker>,

    #[account(mut, seeds = [VAULT_SEED.as_bytes(), vault_mint.key().as_ref()], bump)]
    pub vault_pda: Account<'info, Vault>,

    #[account(mut, token::authority = vault_pda, token::mint = vault_mint)]
    pub vault_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
