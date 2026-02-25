use anchor_lang::prelude::*;
use anchor_spl::{associated_token::AssociatedToken, token::{self, Mint, Token, TokenAccount}};
use crate::{components::calculate_staker_yield, events::StakingEvent, states::{Staker, Admin, Vault}, utils::*};

/// Stake a given amount of vault tokens
///
/// # Arguments
/// * `ctx` - context containing all accounts for staking
/// * `amount` - amount of vault tokens to stake
#[inline(never)]
pub fn staking(ctx: Context<StakingInstructionAccounts>, amount: u64) -> Result<()> {
    require!(amount > 0, OxediumError::ZeroAmount);

    // Capture vault PDA key before taking mutable borrow (borrow checker)
    let vault_pda_key = ctx.accounts.vault_pda.key();

    let vault: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda;
    let staker: &mut Account<'_, Staker> = &mut ctx.accounts.staker_pda;

    // Get the cumulative yield per token from the vault
    let cumulative_yield: u128 = vault.cumulative_yield_per_lp;
    // Get the staker's current staked amount
    let staker_balance: u64 = staker.staked_amount;
    // Get the last recorded cumulative yield for the staker
    let last_cumulative_yield: u128 = staker.last_cumulative_yield;

    // Transfer the staked vault tokens from signer to treasury
    let cpi_accounts = token::Transfer {
        from: ctx.accounts.signer_ata.to_account_info(),
        to: ctx.accounts.treasury_ata.to_account_info(),
        authority: ctx.accounts.signer.to_account_info(),
    };

    token::transfer(CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts), amount)?;

    // Set staker PDA owner and vault PDA key
    staker.owner = ctx.accounts.signer.key();
    staker.vault = vault_pda_key;

    // Calculate pending yield for staker and update
    staker.pending_claim = staker.pending_claim
        .checked_add(calculate_staker_yield(cumulative_yield, staker_balance, last_cumulative_yield))
        .ok_or(OxediumError::OverflowInAdd)?;
    staker.last_cumulative_yield = cumulative_yield;
    staker.staked_amount = staker.staked_amount
        .checked_add(amount)
        .ok_or(OxediumError::OverflowInAdd)?;

    // Update vault liquidity accounting
    vault.initial_balance = vault.initial_balance
        .checked_add(amount)
        .ok_or(OxediumError::OverflowInAdd)?;
    vault.current_balance = vault.current_balance
        .checked_add(amount)
        .ok_or(OxediumError::OverflowInAdd)?;

    emit!(StakingEvent {
        user: ctx.accounts.signer.key(),
        mint: vault.token_mint.key(),
        amount: amount
    });

    Ok(())
}

/// Accounts context for the staking instruction
#[derive(Accounts)]
pub struct StakingInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>, // the user staking tokens

    pub vault_mint: Account<'info, Mint>, // vault token mint

    #[account(mut, token::authority = signer, token::mint = vault_mint)]
    pub signer_ata: Account<'info, TokenAccount>, // user token account for vault token

    #[account(mut, seeds = [VAULT_SEED.as_bytes(), &vault_mint.to_account_info().key.to_bytes()], bump)]
    pub vault_pda: Account<'info, Vault>, // vault PDA storing liquidity and yield info

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [STAKER_SEED.as_bytes(), vault_pda.key().as_ref(), signer.key().as_ref()],
        bump,
        space = 8 + 32 + 32 + 8 + 16 + 8,
    )]
    pub staker_pda: Account<'info, Staker>, // staker PDA storing pending rewards and last yield

    #[account(mut, seeds = [OXEDIUM_SEED.as_bytes(), ADMIN_SEED.as_bytes()], bump)]
    pub treasury_pda: Account<'info, Admin>, // treasury PDA

    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = vault_mint,
        associated_token::authority = treasury_pda,
    )]
    pub treasury_ata: Account<'info, TokenAccount>, // treasury token account holding staked vault tokens

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
