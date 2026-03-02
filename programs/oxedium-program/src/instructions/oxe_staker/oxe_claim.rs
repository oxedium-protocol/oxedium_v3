use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::{
    components::calculate_staker_yield,
    events::OxeClaimEvent,
    states::{OxeCheckpoint, OxeStaker, Vault},
    utils::{OXE_CHECKPOINT_SEED, OXE_STAKER_SEED, OxediumError, VAULT_SEED},
};

/// Claim accumulated OXE staking rewards from a single vault.
/// Rewards are paid in the vault's native token (standard SPL).
pub fn oxe_claim(ctx: Context<OxeClaimInstructionAccounts>) -> Result<()> {
    let vault_pda_info = ctx.accounts.vault_pda.to_account_info();

    let vault = &mut ctx.accounts.vault_pda;
    let checkpoint = &mut ctx.accounts.oxe_checkpoint_pda;
    let staker_balance = ctx.accounts.oxe_staker_pda.staked_amount;

    let earned_since_last = calculate_staker_yield(
        vault.oxe_cumulative_yield_per_staker,
        staker_balance,
        checkpoint.last_oxe_cumulative_yield,
    )?;

    let total_claimable = checkpoint
        .pending_yield
        .checked_add(earned_since_last)
        .ok_or(OxediumError::OverflowInAdd)?;

    require!(total_claimable > 0, OxediumError::ZeroAmount);

    // PDA-signed transfer from vault_ata to signer_ata
    let mint_key = ctx.accounts.token_mint.key();
    let seeds: &[&[u8]] = &[
        VAULT_SEED.as_bytes(),
        mint_key.as_ref(),
        &[ctx.bumps.vault_pda],
    ];
    let signer_seeds: &[&[&[u8]]] = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.vault_ata.to_account_info(),
        to: ctx.accounts.signer_ata.to_account_info(),
        authority: vault_pda_info,
    };
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        ),
        total_claimable,
    )?;

    vault.current_balance = vault
        .current_balance
        .checked_sub(total_claimable)
        .ok_or(OxediumError::OverflowInSub)?;

    // Reset checkpoint
    checkpoint.last_oxe_cumulative_yield = vault.oxe_cumulative_yield_per_staker;
    checkpoint.pending_yield = 0;

    emit!(OxeClaimEvent {
        user: ctx.accounts.signer.key(),
        vault: vault.key(),
        mint: vault.token_mint,
        amount: total_claimable,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct OxeClaimInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        mut,
        token::authority = signer,
        token::mint = token_mint,
    )]
    pub signer_ata: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [VAULT_SEED.as_bytes(), token_mint.key().as_ref()],
        bump,
    )]
    pub vault_pda: Account<'info, Vault>,

    #[account(mut, token::authority = vault_pda, token::mint = token_mint)]
    pub vault_ata: Account<'info, TokenAccount>,

    #[account(
        seeds = [OXE_STAKER_SEED.as_bytes(), signer.key().as_ref()],
        bump,
        constraint = oxe_staker_pda.owner == signer.key() @ OxediumError::InvalidStaker,
    )]
    pub oxe_staker_pda: Account<'info, OxeStaker>,

    #[account(
        mut,
        seeds = [OXE_CHECKPOINT_SEED.as_bytes(), vault_pda.key().as_ref(), signer.key().as_ref()],
        bump,
        constraint = oxe_checkpoint_pda.owner == signer.key() @ OxediumError::InvalidStaker,
        constraint = oxe_checkpoint_pda.vault == vault_pda.key() @ OxediumError::InvalidVault,
    )]
    pub oxe_checkpoint_pda: Account<'info, OxeCheckpoint>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
