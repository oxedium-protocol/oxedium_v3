use anchor_lang::prelude::*;

/// Per-user OXE staking account
/// Seeds: [OXE_STAKER_SEED, owner]
/// Space: 8 + 32 + 8 = 48
#[account]
pub struct OxeStaker {
    pub owner: Pubkey,
    pub oxe_balance: u64,
}
