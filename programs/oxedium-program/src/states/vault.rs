use anchor_lang::prelude::*;

#[account]
pub struct Vault {
    pub base_fee_bps: u64,
    pub protocol_fee_bps: u64,
    pub max_exit_fee_bps: u64,

    pub token_mint: Pubkey,

    pub pyth_price_account: Pubkey,
    pub max_age_price: u64,

    pub initial_balance: u64,
    pub current_balance: u64,
    
    pub cumulative_yield_per_lp: u128,
    /// Cumulative protocol yield per OXE staked (scaled by SCALE).
    /// Increases each swap when OXE stakers exist.
    pub oxe_cumulative_yield_per_staker: u128,
}