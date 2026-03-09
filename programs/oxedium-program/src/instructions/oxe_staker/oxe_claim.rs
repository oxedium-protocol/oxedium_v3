use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount},
};

use crate::{
    components::calculate_staker_yield,
    events::OxeClaimEvent,
    states::{OxeStaker, OxeVaultPosition, Vault},
    utils::{OXE_POSITION_SEED, OXE_STAKER_SEED, VAULT_SEED, OxediumError},
};

/// Claim accumulated protocol-fee yield from a single vault.
///
/// On first call for a (staker, vault) pair the position is created and
/// `last_cumulative_yield` is anchored to the current accumulator, so no
/// retroactive yield is awarded.
///
/// Yield earned before an `oxe_unstake` is preserved in `pending_claim` by
/// the unstake instruction (mirrors the LP staking pattern). This instruction
/// then pays out `pending_claim + newly earned` in one transfer.
pub fn oxe_claim(ctx: Context<OxeClaimInstructionAccounts>) -> Result<()> {
    let vault_pda_info = ctx.accounts.vault_pda.to_account_info();
    let vault_key = ctx.accounts.vault_pda.key();

    let vault: &mut Account<'_, Vault> = &mut ctx.accounts.vault_pda;
    let vault_token_mint = vault.token_mint;

    let oxe_staker: &Account<'_, OxeStaker> = &ctx.accounts.oxe_staker_pda;
    let position: &mut Account<'_, OxeVaultPosition> = &mut ctx.accounts.oxe_position_pda;

    let current_cumulative = vault.oxe_cumulative_yield_per_staker;

    // First-time initialisation: anchor the position to the current accumulator
    // so the staker earns only from this point forward (no retroactive yield).
    if position.owner == Pubkey::default() {
        position.owner = ctx.accounts.signer.key();
        position.vault = vault_key;
        position.last_cumulative_yield = current_cumulative;
        position.pending_claim = 0;
        return Ok(());
    }

    // Yield earned since the last flush using the live balance.
    // If the staker called `oxe_unstake` with this vault in remaining_accounts,
    // earned yield was already saved to `pending_claim` and `last_cumulative_yield`
    // was advanced, so `earned` here will be 0 and only pending_claim is paid out.
    let earned = calculate_staker_yield(
        current_cumulative,
        oxe_staker.oxe_balance,
        position.last_cumulative_yield,
    )?;

    let amount = earned
        .checked_add(position.pending_claim)
        .ok_or(OxediumError::OverflowInAdd)?;

    require!(amount > 0, OxediumError::ZeroAmount);

    let mint_key = ctx.accounts.token_mint.key();
    let seeds = &[VAULT_SEED.as_bytes(), mint_key.as_ref(), &[ctx.bumps.vault_pda]];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = token::Transfer {
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
        amount,
    )?;

    vault.current_balance = vault.current_balance
        .checked_sub(amount)
        .ok_or(OxediumError::OverflowInSub)?;

    // Flush position
    position.last_cumulative_yield = current_cumulative;
    position.pending_claim = 0;

    emit!(OxeClaimEvent {
        user: ctx.accounts.signer.key(),
        vault: vault_key,
        mint: vault_token_mint,
        amount,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct OxeClaimInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        seeds = [OXE_STAKER_SEED.as_bytes(), signer.key().as_ref()],
        bump,
        constraint = oxe_staker_pda.owner == signer.key() @ OxediumError::InvalidStaker,
    )]
    pub oxe_staker_pda: Account<'info, OxeStaker>,

    #[account(
        init_if_needed,
        payer = signer,
        seeds = [OXE_POSITION_SEED.as_bytes(), vault_pda.key().as_ref(), signer.key().as_ref()],
        bump,
        space = 8 + 32 + 32 + 16 + 8,
    )]
    pub oxe_position_pda: Account<'info, OxeVaultPosition>,

    #[account(mut, seeds = [VAULT_SEED.as_bytes(), token_mint.key().as_ref()], bump)]
    pub vault_pda: Account<'info, Vault>,

    /// Destination: signer's ATA for the vault token
    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = token_mint,
        associated_token::authority = signer,
    )]
    pub signer_ata: Account<'info, TokenAccount>,

    /// Source: vault's ATA for the vault token (protocol fees sit here)
    #[account(mut, token::authority = vault_pda, token::mint = token_mint)]
    pub vault_ata: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
