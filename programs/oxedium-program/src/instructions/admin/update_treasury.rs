use crate::{components::check_admin, states::Admin, utils::{ADMIN_SEED, OXEDIUM_SEED}};
use anchor_lang::prelude::*;

#[inline(never)]
pub fn update_treasury(ctx: Context<UpdateTreasuryInstructionAccounts>) -> Result<()> {
    let admin: &mut Account<'_, Admin> = &mut ctx.accounts.admin_pda;

    check_admin(admin, &ctx.accounts.signer)?;

    admin.pubkey = ctx.accounts.new_admin.key();

    msg!("UpdateAdmin {{new_admin: {}}}", admin.pubkey.key());

    Ok(())
}

#[derive(Accounts)]
pub struct UpdateTreasuryInstructionAccounts<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    /// CHECK: No constraints, assumed valid by admin
    pub new_admin: AccountInfo<'info>,

    #[account(mut, seeds = [OXEDIUM_SEED.as_bytes(), ADMIN_SEED.as_bytes()], bump)]
    pub admin_pda: Account<'info, Admin>,

    pub system_program: Program<'info, System>,
}
