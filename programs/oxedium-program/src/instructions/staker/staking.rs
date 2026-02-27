use anchor_lang::prelude::*;
use anchor_spl::{associated_token::AssociatedToken, token::{self, Mint, Token, TokenAccount}};
use crate::{components::calculate_staker_yield, events::StakingEvent, states::{Staker, Vault}, utils::*};

/// Stake a given amount of vault tokens
///
/// # Arguments
/// * `ctx` - context containing all accounts for staking
/// * `amount` - amount of vault tokens to stake
#[inline(never)]
pub fn staking(ctx: Context<StakingInstructionAccounts>, amount: u64) -> Result<()> {
    require!(amount > 0, OxediumError::ZeroAmount);

    let vault_pda_key = ctx.accounts.vault_pda.key();

    let vault: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda;
    let staker: &mut Account<'_, Staker> = &mut ctx.accounts.staker_pda;

    let cumulative_yield: u128 = vault.cumulative_yield_per_lp;
    let staker_balance: u64 = staker.staked_amount;
    let last_cumulative_yield: u128 = staker.last_cumulative_yield;

    let cpi_accounts = token::Transfer {
        from: ctx.accounts.signer_ata.to_account_info(),
        to: ctx.accounts.vault_ata.to_account_info(),
        authority: ctx.accounts.signer.to_account_info(),
    };

    token::transfer(CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts), amount)?;

    staker.owner = ctx.accounts.signer.key();
    staker.vault = vault_pda_key;

    staker.pending_claim = staker.pending_claim
        .checked_add(calculate_staker_yield(cumulative_yield, staker_balance, last_cumulative_yield))
        .ok_or(OxediumError::OverflowInAdd)?;
    staker.last_cumulative_yield = cumulative_yield;
    staker.staked_amount = staker.staked_amount
        .checked_add(amount)
        .ok_or(OxediumError::OverflowInAdd)?;

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
    pub signer: Signer<'info>,

    pub vault_mint: Account<'info, Mint>,

    #[account(mut, token::authority = signer, token::mint = vault_mint)]
    pub signer_ata: Account<'info, TokenAccount>,

    #[account(mut, seeds = [VAULT_SEED.as_bytes(), vault_mint.key().as_ref()], bump)]
    pub vault_pda: Account<'info, Vault>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [STAKER_SEED.as_bytes(), vault_pda.key().as_ref(), signer.key().as_ref()],
        bump,
        space = 8 + 32 + 32 + 8 + 16 + 8,
    )]
    pub staker_pda: Account<'info, Staker>,

    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = vault_mint,
        associated_token::authority = vault_pda,
    )]
    pub vault_ata: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
