use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::{components::check_admin, states::{Admin, Vault}, utils::{ADMIN_SEED, OXEDIUM_SEED, VAULT_SEED, OxediumError}};

pub fn collect(ctx: Context<CollectInstructionAccounts>) -> Result<()> {
    check_admin(&ctx.accounts.admin_pda, &ctx.accounts.signer)?;

    let vault_pda_info = ctx.accounts.vault_pda.to_account_info();

    let vault: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda;

    let protocol_yield = vault.protocol_yield;

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
            signer_seeds
        ),
        protocol_yield
    )?;

    vault.current_balance = vault.current_balance
        .checked_sub(protocol_yield)
        .ok_or(OxediumError::OverflowInSub)?;
    vault.protocol_yield = 0;

    msg!("CollectProtocolYield {{mint: {}, amount: {}}}", vault.token_mint.key(), protocol_yield);

    Ok(())
}

#[derive(Accounts)]
pub struct CollectInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(mut, token::authority = signer, token::mint = token_mint)]
    pub signer_ata: Account<'info, TokenAccount>,

    #[account(mut, seeds = [VAULT_SEED.as_bytes(), token_mint.key().as_ref()], bump)]
    pub vault_pda: Account<'info, Vault>,

    #[account(seeds = [OXEDIUM_SEED.as_bytes(), ADMIN_SEED.as_bytes()], bump)]
    pub admin_pda: Account<'info, Admin>,

    #[account(mut, token::authority = vault_pda, token::mint = token_mint)]
    pub vault_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
