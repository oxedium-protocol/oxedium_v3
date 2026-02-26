use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::{components::{calculate_fee_amount, calculate_staker_yield}, events::UnstakingEvent, states::{Staker, Vault}, utils::*};

#[inline(never)]
pub fn unstaking(ctx: Context<UnstakingInstructionAccounts>, amount: u64) -> Result<()> {
    require!(amount > 0, OxediumError::ZeroAmount);

    // Capture AccountInfo before taking mutable borrow of vault_pda (borrow checker)
    let vault_pda_info = ctx.accounts.vault_pda.to_account_info();

    let vault: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda;
    let staker: &mut Account<'_, Staker> = &mut ctx.accounts.staker_pda;

    require!(staker.staked_amount >= amount, OxediumError::Overflow);

    let cumulative_yield: u128 = vault.cumulative_yield_per_lp;
    let last_cumulative_yield: u128 = staker.last_cumulative_yield;

    // --- Dynamic Fee Logic ---
    let mut unstake_amount = amount;
    // Guard against division by zero when vault is empty (C-01)
    let liquidity_ratio = if vault.initial_balance == 0 {
        100u128
    } else {
        (vault.current_balance as u128 * 100) / vault.initial_balance as u128
    };
    let mut extra_fee_bps: u64 = 0;

    // Apply extra fee if current liquidity < 50%
    if liquidity_ratio < 50 {
        extra_fee_bps = 200; // 2% extra fee if liquidity too low
    }

    if extra_fee_bps > 0 {
        unstake_amount = calculate_fee_amount(unstake_amount, extra_fee_bps, 0)?.0;
    }

    // Transfer unstake amount from vault ATA to staker; vault_pda signs
    let mint_key = ctx.accounts.token_mint.key();
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
        unstake_amount)?;

    // Update pending yield for the staker (C-05: checked_add)
    staker.pending_claim = staker.pending_claim
        .checked_add(calculate_staker_yield(cumulative_yield, staker.staked_amount, last_cumulative_yield))
        .ok_or(OxediumError::OverflowInAdd)?;
    staker.last_cumulative_yield = cumulative_yield;
    staker.staked_amount = staker.staked_amount
        .checked_sub(amount)
        .ok_or(OxediumError::OverflowInSub)?;

    // Update vault liquidity (C-03: both balances decrease by full `amount`;
    // the extra_fee stays in the vault and is credited to protocol_yield)
    vault.initial_balance = vault.initial_balance
        .checked_sub(amount)
        .ok_or(OxediumError::OverflowInSub)?;
    vault.current_balance = vault.current_balance
        .checked_sub(amount)
        .ok_or(OxediumError::OverflowInSub)?;
    let extra_fee = amount - unstake_amount;
    if extra_fee > 0 {
        vault.protocol_yield = vault.protocol_yield
            .checked_add(extra_fee)
            .ok_or(OxediumError::OverflowInAdd)?;
    }

    emit!(UnstakingEvent {
        user: ctx.accounts.signer.key(),
        mint: vault.token_mint.key(),
        amount: unstake_amount,
        extra_fee_bps: extra_fee_bps
    });

    Ok(())
}

/// Accounts required for the unstaking instruction
#[derive(Accounts)]
pub struct UnstakingInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_mint: Account<'info, Mint>, // read-only: not modified

    #[account(mut, token::authority = signer, token::mint = token_mint)]
    pub signer_ata: Account<'info, TokenAccount>,

    #[account(mut, seeds = [VAULT_SEED.as_bytes(), token_mint.key().as_ref()], bump)]
    pub vault_pda: Account<'info, Vault>,

    #[account(
        mut,
        seeds = [STAKER_SEED.as_bytes(), vault_pda.key().as_ref(), signer.key().as_ref()],
        bump,
        constraint = staker_pda.owner == signer.key() @ OxediumError::InvalidAdmin,
        constraint = staker_pda.vault == vault_pda.key() @ OxediumError::InvalidAdmin,
    )]
    pub staker_pda: Account<'info, Staker>,

    #[account(mut, token::authority = vault_pda, token::mint = token_mint)]
    pub vault_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
