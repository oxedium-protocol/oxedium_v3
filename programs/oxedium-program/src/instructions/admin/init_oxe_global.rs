use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint as MintInterface;

use crate::{
    components::check_admin,
    states::{Admin, OxeGlobal},
    utils::{ADMIN_SEED, OXEDIUM_SEED, OXE_GLOBAL_SEED},
};

pub fn init_oxe_global(ctx: Context<InitOxeGlobalInstructionAccounts>) -> Result<()> {
    check_admin(&ctx.accounts.admin_pda, &ctx.accounts.signer)?;

    let oxe_global: &mut Account<'_, OxeGlobal> = &mut ctx.accounts.oxe_global_pda;
    oxe_global.oxe_mint = ctx.accounts.oxe_mint.key();
    oxe_global.total_oxe_staked = 0;

    msg!("InitOxeGlobal {{oxe_mint: {}}}", oxe_global.oxe_mint);

    Ok(())
}

#[derive(Accounts)]
pub struct InitOxeGlobalInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    pub oxe_mint: InterfaceAccount<'info, MintInterface>,

    #[account(
        init,
        payer = signer,
        seeds = [OXEDIUM_SEED.as_bytes(), OXE_GLOBAL_SEED.as_bytes()],
        bump,
        space = 8 + 32 + 8,
    )]
    pub oxe_global_pda: Account<'info, OxeGlobal>,

    #[account(seeds = [OXEDIUM_SEED.as_bytes(), ADMIN_SEED.as_bytes()], bump)]
    pub admin_pda: Account<'info, Admin>,

    pub system_program: Program<'info, System>,
}
