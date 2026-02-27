use crate::{components::check_admin, states::{Vault, Admin}, utils::*};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;

pub fn init_vault(
    ctx: Context<InitVaultInstructionAccounts>,
    base_fee_bps: u64,
    protocol_fee_bps: u64,
    max_age_price: u64,
    max_exit_fee_bps: u64,
) -> Result<()> {
    let vault: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda;

    check_admin(&ctx.accounts.treasury_pda, &ctx.accounts.signer)?;

    require!(base_fee_bps <= 1_000, OxediumError::FeeExceeds);
    require!(protocol_fee_bps <= 500, OxediumError::FeeExceeds);
    require!(max_exit_fee_bps <= 1_000, OxediumError::FeeExceeds);
    require!(max_age_price > 0, OxediumError::InvalidDeviation);

    vault.base_fee_bps = base_fee_bps;
    vault.protocol_fee_bps = protocol_fee_bps;
    vault.max_exit_fee_bps = max_exit_fee_bps;
    vault.token_mint = ctx.accounts.token_mint.key();
    vault.pyth_price_account = ctx.accounts.pyth_price_account.key();
    vault.max_age_price = max_age_price;
    vault.initial_balance = 0;
    vault.current_balance = 0;
    vault.cumulative_yield_per_lp = 0;
    vault.protocol_yield = 0;

    Ok(())
}

#[derive(Accounts)]
pub struct InitVaultInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    pub pyth_price_account: Account<'info, PriceUpdateV2>,

    #[account(
        init,
        payer = signer,
        seeds = [VAULT_SEED.as_bytes(), token_mint.key().as_ref()],
        bump,
        space = 8 + 8 + 8 + 8 + 32 + 32 + 8 + 8 + 8 + 16 + 8,
    )]
    pub vault_pda: Account<'info, Vault>,

    #[account(mut, seeds = [OXEDIUM_SEED.as_bytes(), ADMIN_SEED.as_bytes()], bump)]
    pub treasury_pda: Account<'info, Admin>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
