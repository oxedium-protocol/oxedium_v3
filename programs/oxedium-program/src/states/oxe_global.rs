use anchor_lang::prelude::*;

/// Global OXE staking state — singleton PDA
/// Seeds: [OXEDIUM_SEED, OXE_GLOBAL_SEED]
/// Space: 8 + 32 + 8 = 48
#[account]
pub struct OxeGlobal {
    /// The SPL mint for the OXE governance token
    pub oxe_mint: Pubkey,
    /// Total amount of OXE currently staked across all stakers
    pub total_oxe_staked: u64,
}
