use anchor_lang::prelude::*;
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::Mint;

use crate::{
    components::check_admin,
    states::{Admin, OxeGlobalState},
    utils::{ADMIN_SEED, OXE_GLOBAL_SEED, OXEDIUM_SEED},
};

/// Initialize the global OXE staking state.
/// Admin-only, one-time call. Records the Token22 OXE mint.
pub fn init_oxe_global(ctx: Context<InitOxeGlobalInstructionAccounts>) -> Result<()> {
    check_admin(&ctx.accounts.admin_pda, &ctx.accounts.signer)?;

    let state = &mut ctx.accounts.oxe_global_state;
    state.oxe_mint = ctx.accounts.oxe_mint.key();
    state.total_staked = 0;

    msg!("InitOxeGlobal {{mint: {}}}", state.oxe_mint);

    Ok(())
}

#[derive(Accounts)]
pub struct InitOxeGlobalInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub oxe_mint: InterfaceAccount<'info, Mint>,

    #[account(seeds = [OXEDIUM_SEED.as_bytes(), ADMIN_SEED.as_bytes()], bump)]
    pub admin_pda: Account<'info, Admin>,

    #[account(
        init,
        payer = signer,
        seeds = [OXE_GLOBAL_SEED.as_bytes()],
        bump,
        space = 8 + 32 + 8,
    )]
    pub oxe_global_state: Account<'info, OxeGlobalState>,

    pub token_program_2022: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}
