#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

use instructions::admin::*;
use instructions::staker::*;
use instructions::trader::*;

pub mod states;
pub mod instructions;
pub mod components;
pub mod utils;
pub mod events;


declare_id!("oxe3zgfkaGph4X5bv4RZRFUcQXZcXxK9fxDizCtPZv7");

#[program]
pub mod oxedium_program {
    use super::*;

    pub fn init_admin(ctx: Context<InitAdminInstructionAccounts>) -> Result<()> {
        instructions::admin::init_admin(ctx)
    }

    pub fn update_admin(ctx: Context<UpdateAdminInstructionAccounts>) -> Result<()> {
        instructions::admin::update_admin(ctx)
    }

    pub fn init_vault(ctx: Context<InitVaultInstructionAccounts>, base_fee: u64, max_age_price: u64, max_exit_fee_bps: u64) -> Result<()> {
        instructions::admin::init_vault(ctx, base_fee, max_age_price, max_exit_fee_bps)
    }

    pub fn update_vault(ctx: Context<UpdateVaultInstructionAccounts>, base_fee_bps: u64, protocol_fee_bps: u64, max_age_price: u64, max_exit_fee_bps: u64) -> Result<()> {
        instructions::admin::update_vault(ctx, base_fee_bps, protocol_fee_bps, max_age_price, max_exit_fee_bps)
    }

    pub fn collect(ctx: Context<CollectInstructionAccounts>) -> Result<()> {
        instructions::admin::collect(ctx)
    }

    // Staker instructions
    pub fn staking(ctx: Context<StakingInstructionAccounts>, amount: u64) -> Result<()> {
        instructions::staker::staking(ctx, amount)
    }

    pub fn unstaking(ctx: Context<UnstakingInstructionAccounts>, amount: u64) -> Result<()> {
        instructions::staker::unstaking(ctx, amount)
    }

    pub fn claim(ctx: Context<ClaimInstructionAccounts>) -> Result<()> {
        instructions::staker::claim(ctx)
    }

    // Trader instruction
    pub fn swap(ctx: Context<SwapInstructionAccounts>, amount_in: u64, minimum_out: u64) -> Result<()> {
        instructions::trader::swap(ctx, amount_in, minimum_out)
    }
}
