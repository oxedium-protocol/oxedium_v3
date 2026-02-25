use anchor_lang::prelude::*;

#[account]
pub struct Staker {
    pub owner: Pubkey,
    pub vault: Pubkey,
    pub staked_amount: u64,
    pub last_cumulative_yield: u128,
    pub pending_claim: u64
}