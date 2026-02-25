use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::{components::calculate_staker_yield, events::ClaimEvent, states::{Staker, Admin, Vault}, utils::{OXEDIUM_SEED, STAKER_SEED, ADMIN_SEED, VAULT_SEED, OxediumError}};

/// Claim accumulated yield for a staker from a vault
///
/// # Arguments
/// * `ctx` - context containing all accounts required for claiming
pub fn claim(ctx: Context<ClaimInstructionAccounts>) -> Result<()> {
    let vault: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda;
    let staker: &mut Account<'_, Staker> = &mut ctx.accounts.staker_pda;

    // Get cumulative yield per token from the vault
    let cumulative_yield_per_lp: u128 = vault.cumulative_yield_per_lp;
    // Get the staker's current staked amount
    let staker_balance: u64 = staker.staked_amount;
    // Get the last cumulative yield recorded for the staker
    let staker_last_cumulative_yield: u128 = staker.last_cumulative_yield;
    // Get the pending claim for the staker
    let staker_pending_claim: u64 = staker.pending_claim;

    // Calculate total yield: new yield + pending claim (C-05: checked_add)
    let staker_yield: u64 = calculate_staker_yield(cumulative_yield_per_lp, staker_balance, staker_last_cumulative_yield);
    let amount: u64 = staker_yield
        .checked_add(staker_pending_claim)
        .ok_or(OxediumError::OverflowInAdd)?;

    require!(amount > 0, OxediumError::ZeroAmount);

    // PDA seeds for signing transfer from treasury
    let seeds = &[OXEDIUM_SEED.as_bytes(), ADMIN_SEED.as_bytes(), &[ctx.bumps.treasury_pda]];
    let signer_seeds = &[&seeds[..]];

    // Define CPI transfer from treasury to staker
    let cpi_accounts = Transfer {
        from: ctx.accounts.treasury_ata.to_account_info(),
        to: ctx.accounts.signer_ata.to_account_info(),
        authority: ctx.accounts.treasury_pda.to_account_info()
    };

    // Execute the transfer using PDA signer
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds),
        amount)?;

    // Update staker PDA state
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
    pub signer: Signer<'info>, // staker claiming rewards

    /// Vault token mint
    pub vault_mint: Account<'info, Mint>,

    /// Staker's vault token account to receive claimed yield
    #[account(mut, token::authority = signer, token::mint = vault_mint)]
    pub signer_ata: Account<'info, TokenAccount>,

    /// Staker PDA storing last yield and pending claim
    #[account(
        mut,
        seeds = [STAKER_SEED.as_bytes(), vault_pda.key().as_ref(), signer.key().as_ref()],
        bump,
        constraint = staker_pda.owner == signer.key() @ OxediumError::InvalidAdmin,
        constraint = staker_pda.vault == vault_pda.key() @ OxediumError::InvalidAdmin,
    )]
    pub staker_pda: Account<'info, Staker>,

    /// Vault PDA storing cumulative yield and liquidity
    #[account(mut, seeds = [VAULT_SEED.as_bytes(), vault_mint.key().as_ref()], bump)]
    pub vault_pda: Account<'info, Vault>,

    /// Treasury PDA used to sign yield transfer
    #[account(mut, seeds = [OXEDIUM_SEED.as_bytes(), ADMIN_SEED.as_bytes()], bump)]
    pub treasury_pda: Account<'info, Admin>,

    /// Treasury token account holding protocol/staker funds
    #[account(
        mut,
        associated_token::mint = vault_mint,
        associated_token::authority = treasury_pda,
    )]
    pub treasury_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
