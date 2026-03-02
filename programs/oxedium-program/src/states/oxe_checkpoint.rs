use anchor_lang::prelude::*;

/// Per-user per-vault checkpoint for OXE staking rewards.
/// Tracks the last-seen accumulator value and buffered pending yield.
/// Seeds: ["oxe-checkpoint-seed", vault_pda, owner]
/// Space: 8 (disc) + 32 (owner) + 32 (vault) + 16 (last_oxe_cumulative_yield) + 8 (pending_yield) = 96
#[account]
pub struct OxeCheckpoint {
    pub owner: Pubkey,
    pub vault: Pubkey,
    /// Snapshot of vault.oxe_cumulative_yield_per_staker at the last interaction
    pub last_oxe_cumulative_yield: u128,
    /// Yield accrued up to the last snapshot (pending claim)
    pub pending_yield: u64,
}
