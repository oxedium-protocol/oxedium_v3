use crate::{states::Admin, utils::{ADMIN_SEED, OXEDIUM_SEED, OxediumError}};
use anchor_lang::prelude::*;
use std::str::FromStr;

pub fn init_admin(ctx: Context<InitAdminInstructionAccounts>) -> Result<()> {
    let admin_key = Pubkey::from_str("3gXnk9LTHHtFzKK5pkKzp58okeo9V72MjGSyzFUCvKk2")
        .map_err(|_| OxediumError::InvalidAdmin)?; // ensure the key is valid

    if ctx.accounts.signer.key() != admin_key {
        return Err(OxediumError::InvalidAdmin.into());
    }

    let admin: &mut Account<'_, Admin> = &mut ctx.accounts.admin_pda;

    admin.pubkey = ctx.accounts.signer.key();

    msg!("InitAdmin {{new_admin: {}}}", admin.pubkey.key());

    Ok(())
}

#[derive(Accounts)]
pub struct InitAdminInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        init,
        payer = signer,
        seeds = [OXEDIUM_SEED.as_bytes(), ADMIN_SEED.as_bytes()],
        bump,
        space = 8 + 32,
    )]
    pub admin_pda: Account<'info, Admin>,

    pub system_program: Program<'info, System>,
}
