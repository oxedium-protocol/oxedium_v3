use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount},
};
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;

use crate::{
    components::compute_swap_math,
    events::SwapEvent,
    states::Vault,
    utils::{OxediumError, SCALE, VAULT_SEED},
};

/// Swap tokens from one vault to another
///
/// # Arguments
/// * `ctx` - context containing all accounts
/// * `amount_in` - amount of input tokens from user
/// * `minimum_out` - minimum amount output
pub fn swap(
    ctx: Context<SwapInstructionAccounts>,
    amount_in: u64,
    minimum_out: u64,
) -> Result<()> {
    require!(amount_in > 0, OxediumError::ZeroAmount);
    require!(ctx.accounts.token_mint_in.key() != ctx.accounts.token_mint_out.key(), OxediumError::SameMint);

    let vault_pda_out_info = ctx.accounts.vault_pda_out.to_account_info();

    let vault_in: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda_in;
    let vault_out: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda_out;

    if ctx.accounts.pyth_price_account_in.key() != vault_in.pyth_price_account {
        return Err(OxediumError::InvalidPythAccount.into());
    }
    if ctx.accounts.pyth_price_account_out.key() != vault_out.pyth_price_account {
        return Err(OxediumError::InvalidPythAccount.into());
    }

    let oracle_in: Account<'_, PriceUpdateV2>  = ctx.accounts.pyth_price_account_in.clone();
    let oracle_out: Account<'_, PriceUpdateV2>  = ctx.accounts.pyth_price_account_out.clone();

    let clock: Clock = Clock::get()?;
    let current_timestamp: i64 = clock.unix_timestamp;

    let publish_time_in = ctx.accounts.pyth_price_account_in.price_message.publish_time;
    let publish_time_out = ctx.accounts.pyth_price_account_out.price_message.publish_time;

    if publish_time_in > current_timestamp || publish_time_out > current_timestamp {
        return Err(OxediumError::OracleDataTooOld.into());
    }

    let max_age_vault_in = current_timestamp - publish_time_in;
    let max_age_vault_out = current_timestamp - publish_time_out;

    if max_age_vault_in > vault_in.max_age_price as i64 {
        msg!("Vault In: Price feed stale by {} seconds", max_age_vault_in);
        return Err(OxediumError::OracleDataTooOld.into());
    }
    if max_age_vault_out > vault_out.max_age_price as i64 {
        msg!(
            "Vault Out: Price feed stale by {} seconds",
            max_age_vault_out
        );
        return Err(OxediumError::OracleDataTooOld.into());
    }

    let result = compute_swap_math(
        amount_in,
        oracle_in.price_message,
        oracle_out.price_message,
        ctx.accounts.token_mint_in.decimals,
        ctx.accounts.token_mint_out.decimals,
        vault_in,
        vault_out
    )?;

    if result.net_amount_out < minimum_out {
        return Err(OxediumError::HighSlippage.into());
    }

    vault_in.current_balance = vault_in.current_balance
        .checked_add(amount_in)
        .ok_or(OxediumError::OverflowInAdd)?;
    vault_out.current_balance = vault_out.current_balance
        .checked_sub(result.net_amount_out)
        .ok_or(OxediumError::OverflowInSub)?;
    if vault_out.initial_balance > 0 {
        vault_out.cumulative_yield_per_lp = vault_out.cumulative_yield_per_lp
            .checked_add((result.lp_fee_amount as u128 * SCALE) / vault_out.initial_balance as u128)
            .ok_or(OxediumError::OverflowInAdd)?;
        vault_out.protocol_yield = vault_out.protocol_yield
            .checked_add(result.protocol_fee_amount)
            .ok_or(OxediumError::OverflowInAdd)?;
    } else {
        vault_out.protocol_yield = vault_out.protocol_yield
            .checked_add(result.lp_fee_amount)
            .and_then(|v| v.checked_add(result.protocol_fee_amount))
            .ok_or(OxediumError::OverflowInAdd)?;
    }

    let cpi_accounts: token::Transfer<'_> = token::Transfer {
        from: ctx.accounts.signer_ata_in.to_account_info(),
        to: ctx.accounts.vault_ata_in.to_account_info(),
        authority: ctx.accounts.signer.to_account_info(),
    };
    token::transfer(
        CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts),
        amount_in,
    )?;

    let mint_out_key = ctx.accounts.token_mint_out.key();
    let seeds: &[&[u8]; 3] = &[
        VAULT_SEED.as_bytes(),
        mint_out_key.as_ref(),
        &[ctx.bumps.vault_pda_out],
    ];
    let signer_seeds: &[&[&[u8]]; 1] = &[&seeds[..]];

    let cpi_accounts_out: token::Transfer<'_> = token::Transfer {
        from: ctx.accounts.vault_ata_out.to_account_info(),
        to: ctx.accounts.signer_ata_out.to_account_info(),
        authority: vault_pda_out_info,
    };
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts_out,
            signer_seeds,
        ),
        result.net_amount_out,
    )?;

    emit!(SwapEvent {
        user: ctx.accounts.signer.key(),
        fee_bps: result.swap_fee_bps,
        token_in: vault_in.token_mint,
        token_out: vault_out.token_mint,
        amount_in: amount_in,
        amount_out: result.net_amount_out,
        price_in: oracle_in.price_message.price.unsigned_abs(),
        price_out: oracle_out.price_message.price.unsigned_abs(),
        lp_fee: result.lp_fee_amount,
        protocol_fee: result.protocol_fee_amount
    });

    Ok(())
}

/// Accounts required for the swap instruction
#[derive(Accounts)]
pub struct SwapInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_mint_in: Account<'info, Mint>,
    pub token_mint_out: Account<'info, Mint>,

    pub pyth_price_account_in: Account<'info, PriceUpdateV2>,
    pub pyth_price_account_out: Account<'info, PriceUpdateV2>,

    #[account(mut, token::authority = signer, token::mint = token_mint_in)]
    pub signer_ata_in: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = token_mint_out,
        associated_token::authority = signer,
    )]
    pub signer_ata_out: Account<'info, TokenAccount>,

    #[account(mut, seeds = [VAULT_SEED.as_bytes(), token_mint_in.key().as_ref()], bump)]
    pub vault_pda_in: Account<'info, Vault>,

    #[account(mut, seeds = [VAULT_SEED.as_bytes(), token_mint_out.key().as_ref()], bump)]
    pub vault_pda_out: Account<'info, Vault>,

    #[account(mut, token::authority = vault_pda_in, token::mint = token_mint_in)]
    pub vault_ata_in: Account<'info, TokenAccount>,

    #[account(mut, token::authority = vault_pda_out, token::mint = token_mint_out)]
    pub vault_ata_out: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
