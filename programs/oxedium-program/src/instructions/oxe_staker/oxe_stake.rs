use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount},
};

use crate::{
    events::OxeStakeEvent,
    states::{OxeGlobal, OxeStaker},
    utils::{OXEDIUM_SEED, OXE_GLOBAL_SEED, OXE_STAKER_SEED, OxediumError},
};

pub fn oxe_stake(ctx: Context<OxeStakeInstructionAccounts>, amount: u64) -> Result<()> {
    require!(amount > 0, OxediumError::ZeroAmount);

    let oxe_global: &mut Account<'_, OxeGlobal> = &mut ctx.accounts.oxe_global_pda;
    let oxe_staker: &mut Account<'_, OxeStaker> = &mut ctx.accounts.oxe_staker_pda;

    let cpi_accounts = token::Transfer {
        from: ctx.accounts.signer_ata.to_account_info(),
        to: ctx.accounts.oxe_global_ata.to_account_info(),
        authority: ctx.accounts.signer.to_account_info(),
    };
    token::transfer(
        CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts),
        amount,
    )?;

    oxe_staker.owner = ctx.accounts.signer.key();
    oxe_staker.oxe_balance = oxe_staker.oxe_balance
        .checked_add(amount)
        .ok_or(OxediumError::OverflowInAdd)?;

    oxe_global.total_oxe_staked = oxe_global.total_oxe_staked
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

    #[account(
        mut,
        seeds = [OXEDIUM_SEED.as_bytes(), OXE_GLOBAL_SEED.as_bytes()],
        bump,
    )]
    pub oxe_global_pda: Account<'info, OxeGlobal>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [OXE_STAKER_SEED.as_bytes(), signer.key().as_ref()],
        bump,
        space = 8 + 32 + 8,
    )]
    pub oxe_staker_pda: Account<'info, OxeStaker>,

    /// OXE token account of the signer
    #[account(mut, token::authority = signer, token::mint = oxe_global_pda.oxe_mint)]
    pub signer_ata: Account<'info, TokenAccount>,

    /// Program-owned escrow ATA for staked OXE (authority = oxe_global_pda)
    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = oxe_mint,
        associated_token::authority = oxe_global_pda,
    )]
    pub oxe_global_ata: Account<'info, TokenAccount>,

    #[account(address = oxe_global_pda.oxe_mint)]
    pub oxe_mint: Account<'info, Mint>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
