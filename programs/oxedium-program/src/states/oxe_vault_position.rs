use anchor_lang::prelude::*;

/// Tracks an OXE staker's yield position for a specific vault.
/// Created lazily on first claim from a given vault.
/// Seeds: [OXE_POSITION_SEED, vault, owner]
/// Space: 8 + 32 + 32 + 16 + 8 = 96
#[account]
pub struct OxeVaultPosition {
    pub owner: Pubkey,
    pub vault: Pubkey,
    /// Vault's oxe_cumulative_yield_per_staker at the last flush
    pub last_cumulative_yield: u128,
    /// Yield earned since the last flush but not yet transferred.
    /// Populated by `oxe_unstake` (which mirrors LP unstaking) so that
    /// yield accumulated before an unstake is never lost.
    pub pending_claim: u64,
}
