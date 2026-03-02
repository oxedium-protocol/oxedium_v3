use anchor_lang::prelude::*;

/// Per-user OXE staking position.
/// Seeds: ["oxe-staker-seed", owner]
/// Space: 8 (disc) + 32 (owner) + 8 (staked_amount) = 48
#[account]
pub struct OxeStaker {
    pub owner: Pubkey,
    /// Amount of OXE tokens currently staked
    pub staked_amount: u64,
}
