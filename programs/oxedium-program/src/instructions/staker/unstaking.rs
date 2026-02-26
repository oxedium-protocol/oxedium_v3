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

    // --- Dynamic Exit Fee (quadratic curve) ---
    //
    // health  = current_balance / initial_balance  (clamped 0..100)
    // deficit = 100 - health                       (0 at full health, 100 at empty)
    // curved  = deficit² / 100                    (quadratic: small drawdowns → tiny fee,
    //                                               large drawdowns → aggressive fee)
    // exit_fee_bps = max_exit_fee_bps × curved / 100
    //
    // Examples with max_exit_fee_bps = 500 (5% max):
    //   health 100% →   0 bps (0.00%)
    //   health  80% →  20 bps (0.20%)
    //   health  50% → 125 bps (1.25%)
    //   health  20% → 320 bps (3.20%)
    //   health   0% → 500 bps (5.00%)
    let mut unstake_amount = amount;
    let health = if vault.initial_balance == 0 {
        100u128
    } else {
        (vault.current_balance as u128 * 100) / vault.initial_balance as u128
    };
    let deficit = 100u128.saturating_sub(health);
    let curved  = deficit * deficit / 100;
    let exit_fee_bps = (vault.max_exit_fee_bps as u128 * curved / 100) as u64;

    if exit_fee_bps > 0 {
        unstake_amount = calculate_fee_amount(unstake_amount, exit_fee_bps, 0)?.0;
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

    // Update vault liquidity:
    //   initial_balance decreases by the full requested amount (LP share removed).
    //   current_balance decreases only by unstake_amount — the exit fee physically
    //   remains in the ATA and is distributed to the remaining LP stakers.
    vault.initial_balance = vault.initial_balance
        .checked_sub(amount)
        .ok_or(OxediumError::OverflowInSub)?;
    vault.current_balance = vault.current_balance
        .checked_sub(unstake_amount)
        .ok_or(OxediumError::OverflowInSub)?;

    // Distribute exit fee to remaining LP stakers.
    // initial_balance is already decremented, so the exiting staker is excluded.
    // If no LPs remain, the fee is implicitly absorbed into the vault ATA (edge case).
    let exit_fee = amount - unstake_amount;
    if exit_fee > 0 && vault.initial_balance > 0 {
        vault.cumulative_yield_per_lp = vault.cumulative_yield_per_lp
            .checked_add((exit_fee as u128 * SCALE) / vault.initial_balance as u128)
            .ok_or(OxediumError::OverflowInAdd)?;
    }

    emit!(UnstakingEvent {
        user: ctx.accounts.signer.key(),
        mint: vault.token_mint.key(),
        amount: unstake_amount,
        extra_fee_bps: exit_fee_bps
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
