use anchor_lang::prelude::*;
use anchor_lang::AccountSerialize;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount},
};

use crate::{
    components::calculate_staker_yield,
    events::OxeUnstakeEvent,
    states::{OxeGlobal, OxeStaker, OxeVaultPosition, Vault},
    utils::{OXEDIUM_SEED, OXE_GLOBAL_SEED, OXE_STAKER_SEED, OxediumError},
};

/// Unstake OXE tokens and optionally flush pending yield for active vault positions.
///
/// `remaining_accounts` must be provided in pairs: `[vault_pda, oxe_position_pda, ...]`
/// for every vault the staker has an active position in.  For each pair, accumulated
/// yield is computed using the **pre-unstake** balance and saved to `pending_claim`
/// (mirroring the LP unstaking pattern), so it can be collected via `oxe_claim` later.
///
/// Vault positions not included in `remaining_accounts` will lose unclaimed yield
/// once the balance drops to zero.  Clients should pass all active positions.
pub fn oxe_unstake(ctx: Context<OxeUnstakeInstructionAccounts>, amount: u64) -> Result<()> {
    require!(amount > 0, OxediumError::ZeroAmount);

    let oxe_staker: &mut Account<'_, OxeStaker> = &mut ctx.accounts.oxe_staker_pda;

    require!(oxe_staker.oxe_balance >= amount, OxediumError::InsufficientBalance);

    // ── Flush vault positions BEFORE reducing balance ────────────────────────
    // remaining_accounts layout: [vault_pda_0, position_pda_0, vault_pda_1, …]
    let balance_before = oxe_staker.oxe_balance;

    require!(ctx.remaining_accounts.len() % 2 == 0, OxediumError::InvalidVault);

    for i in (0..ctx.remaining_accounts.len()).step_by(2) {
        let vault_info    = &ctx.remaining_accounts[i];
        let position_info = &ctx.remaining_accounts[i + 1];

        require!(position_info.is_writable, OxediumError::InvalidVault);

        // Validate program ownership before deserialising
        require!(vault_info.owner    == &crate::ID, OxediumError::InvalidVault);
        require!(position_info.owner == &crate::ID, OxediumError::InvalidVault);

        // Deserialise without lifetime-coupling to ctx
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

        // Validate linkage
        require!(
            position.owner == ctx.accounts.signer.key(),
            OxediumError::InvalidStaker
        );
        require!(
            position.vault == vault_info.key(),
            OxediumError::InvalidVault
        );

        // Capture yield earned up to this moment using the pre-unstake balance
        let earned = calculate_staker_yield(
            vault.oxe_cumulative_yield_per_staker,
            balance_before,
            position.last_cumulative_yield,
        )?;

        position.pending_claim = position.pending_claim
            .checked_add(earned)
            .ok_or(OxediumError::OverflowInAdd)?;
        position.last_cumulative_yield = vault.oxe_cumulative_yield_per_staker;

        // Write back (remaining_accounts bypass Anchor's auto-serialise)
        let mut data = position_info.try_borrow_mut_data()?;
        position.try_serialize(&mut data.as_mut())?;
    }

    // ── Reduce balances ──────────────────────────────────────────────────────
    oxe_staker.oxe_balance = oxe_staker.oxe_balance
        .checked_sub(amount)
        .ok_or(OxediumError::OverflowInSub)?;

    let oxe_global: &mut Account<'_, OxeGlobal> = &mut ctx.accounts.oxe_global_pda;
    oxe_global.total_oxe_staked = oxe_global.total_oxe_staked
        .checked_sub(amount)
        .ok_or(OxediumError::OverflowInSub)?;

    // ── Transfer OXE back from escrow to signer ──────────────────────────────
    let oxe_global_seeds: &[&[u8]; 3] = &[
        OXEDIUM_SEED.as_bytes(),
        OXE_GLOBAL_SEED.as_bytes(),
        &[ctx.bumps.oxe_global_pda],
    ];
    let signer_seeds = &[&oxe_global_seeds[..]];

    let cpi_accounts = token::Transfer {
        from: ctx.accounts.oxe_global_ata.to_account_info(),
        to: ctx.accounts.signer_ata.to_account_info(),
        authority: ctx.accounts.oxe_global_pda.to_account_info(),
    };
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        ),
        amount,
    )?;

    emit!(OxeUnstakeEvent {
        user: ctx.accounts.signer.key(),
        amount,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct OxeUnstakeInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut,
        seeds = [OXEDIUM_SEED.as_bytes(), OXE_GLOBAL_SEED.as_bytes()],
        bump,
    )]
    pub oxe_global_pda: Account<'info, OxeGlobal>,

    #[account(
        mut,
        seeds = [OXE_STAKER_SEED.as_bytes(), signer.key().as_ref()],
        bump,
        constraint = oxe_staker_pda.owner == signer.key() @ OxediumError::InvalidStaker,
    )]
    pub oxe_staker_pda: Account<'info, OxeStaker>,

    /// OXE token account of the signer (destination)
    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = oxe_mint,
        associated_token::authority = signer,
    )]
    pub signer_ata: Account<'info, TokenAccount>,

    /// Program-owned escrow ATA (source)
    #[account(mut, token::authority = oxe_global_pda, token::mint = oxe_mint)]
    pub oxe_global_ata: Account<'info, TokenAccount>,

    #[account(address = oxe_global_pda.oxe_mint)]
    pub oxe_mint: Account<'info, Mint>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
