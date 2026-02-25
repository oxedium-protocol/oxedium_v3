use crate::{components::check_admin, states::{Admin, Vault}, utils::{OXEDIUM_SEED, ADMIN_SEED, VAULT_SEED, OxediumError}};
use anchor_lang::prelude::*;
use anchor_spl::token::Mint;
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;

pub fn update_vault(
    ctx: Context<UpdateVaultInstructionAccounts>,
    base_fee_bps: u64,
    protocol_fee_bps: u64,
    max_age_price: u64,
) -> Result<()> {
    let vault: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda;

    check_admin(&ctx.accounts.treasury_pda, &ctx.accounts.signer)?;

    require!(base_fee_bps <= 1_000, OxediumError::FeeExceeds);     // max 10%
    require!(protocol_fee_bps <= 500, OxediumError::FeeExceeds);   // max 5%
    require!(max_age_price > 0, OxediumError::InvalidDeviation);

    vault.base_fee_bps = base_fee_bps;
    vault.protocol_fee_bps = protocol_fee_bps;
    vault.pyth_price_account = ctx.accounts.pyth_price_account.key();
    vault.max_age_price = max_age_price;

    msg!("UpdateVault {{mint: {}, base_fee: {}, max_age_price: {}}}", 
        vault.token_mint.key(), 
        vault.base_fee_bps,
        vault.max_age_price
    );

    Ok(())
}

#[derive(Accounts)]
pub struct UpdateVaultInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    /// CHECK: no additional constraints, assumed valid
    pub token_mint: Account<'info, Mint>,

    pub pyth_price_account: Account<'info, PriceUpdateV2>,

    #[account(mut, seeds = [VAULT_SEED.as_bytes(), token_mint.key().as_ref()], bump)]
    pub vault_pda: Account<'info, Vault>,

    #[account(mut, seeds = [OXEDIUM_SEED.as_bytes(), ADMIN_SEED.as_bytes()], bump)]
    pub treasury_pda: Account<'info, Admin>,
    
    pub system_program: Program<'info, System>,
}
