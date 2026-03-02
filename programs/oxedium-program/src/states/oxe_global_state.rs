use anchor_lang::prelude::*;

/// Global singleton PDA for OXE staking.
/// Tracks the OXE Token22 mint and total staked supply.
/// Seeds: ["oxe-global-seed"]
/// Space: 8 (disc) + 32 (oxe_mint) + 8 (total_staked) = 48
#[account]
pub struct OxeGlobalState {
    /// The Token22 mint of the OXE token
    pub oxe_mint: Pubkey,
    /// Total OXE tokens staked across all stakers (denominator in the per-vault accumulator)
    pub total_staked: u64,
}
