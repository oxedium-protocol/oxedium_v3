use anchor_lang::prelude::*;
use anchor_lang::AccountSerialize;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount},
};

use crate::{
    components::calculate_staker_yield,
    events::OxeStakeEvent,
    states::{OxeGlobal, OxeStaker, OxeVaultPosition, Vault},
    utils::{OXEDIUM_SEED, OXE_GLOBAL_SEED, OXE_STAKER_SEED, OxediumError},
};

/// Stake OXE tokens.
///
/// `remaining_accounts` must be provided in pairs: `[vault_pda, oxe_position_pda, ...]`
/// for every vault the staker has an active position in.  For each pair, accumulated
/// yield is computed using the **pre-stake** balance and saved to `pending_claim`,
/// so the new balance only earns yield from this point forward.
///
/// Vault positions not included in `remaining_accounts` will receive retroactive
/// yield for the pre-stake period at the new (higher) balance.  Clients should
/// pass all active positions.
pub fn oxe_stake(ctx: Context<OxeStakeInstructionAccounts>, amount: u64) -> Result<()> {
    require!(amount > 0, OxediumError::ZeroAmount);

    let oxe_staker: &mut Account<'_, OxeStaker> = &mut ctx.accounts.oxe_staker_pda;

    // ── Flush vault positions BEFORE increasing balance ──────────────────────
    // remaining_accounts layout: [vault_pda_0, position_pda_0, vault_pda_1, …]
    let balance_before = oxe_staker.oxe_balance;

    require!(ctx.remaining_accounts.len() % 2 == 0, OxediumError::InvalidVault);

    for i in (0..ctx.remaining_accounts.len()).step_by(2) {
        let vault_info    = &ctx.remaining_accounts[i];
        let position_info = &ctx.remaining_accounts[i + 1];

        require!(position_info.is_writable, OxediumError::InvalidVault);

        require!(vault_info.owner    == &crate::ID, OxediumError::InvalidVault);
        require!(position_info.owner == &crate::ID, OxediumError::InvalidVault);

        let vault = {
            let data = vault_info.try_borrow_data()?;
            let mut slice: &[u8] = &data;
            Vault::try_deserialize(&mut slice)?
        };
        let mut position = {
            let data = position_info.try_borrow_data()?;
            let mut slice: &[u8] = &data;
            OxeVaultPosition::try_deserialize(&mut slice)?
        };

        require!(
            position.owner == ctx.accounts.signer.key(),
            OxediumError::InvalidStaker
        );
        require!(
            position.vault == vault_info.key(),
            OxediumError::InvalidVault
        );

        let earned = calculate_staker_yield(
            vault.oxe_cumulative_yield_per_staker,
            balance_before,
            position.last_cumulative_yield,
        )?;

        position.pending_claim = position.pending_claim
            .checked_add(earned)
            .ok_or(OxediumError::OverflowInAdd)?;
        position.last_cumulative_yield = vault.oxe_cumulative_yield_per_staker;

        let mut data = position_info.try_borrow_mut_data()?;
        position.try_serialize(&mut data.as_mut())?;
    }

    // ── Transfer OXE to escrow and increase balances ─────────────────────────
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

    let oxe_global: &mut Account<'_, OxeGlobal> = &mut ctx.accounts.oxe_global_pda;
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
