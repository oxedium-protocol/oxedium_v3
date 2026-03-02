use anchor_lang::prelude::*;

use crate::{
    components::calculate_staker_yield,
    utils::OxediumError,
};

// Byte offsets within Vault account data (discriminator included):
// disc(8) + base_fee_bps(8) + protocol_fee_bps(8) + max_exit_fee_bps(8)
// + token_mint(32) + pyth_price_account(32) + max_age_price(8)
// + initial_balance(8) + current_balance(8)
// + cumulative_yield_per_lp(16)
// + oxe_cumulative_yield_per_staker(16)  ← at offset 136
const VAULT_OXE_CUMULATIVE_OFFSET: usize = 136;

// Byte offsets within OxeCheckpoint account data (discriminator included):
// disc(8) + owner(32) + vault(32) + last_oxe_cumulative_yield(16) + pending_yield(8)
const CHECKPOINT_OWNER_OFFSET: usize = 8;
const CHECKPOINT_VAULT_OFFSET: usize = 40;
const CHECKPOINT_LAST_OFFSET: usize = 72;
const CHECKPOINT_PENDING_OFFSET: usize = 88;

/// Iterates `remaining_accounts` as pairs of (vault_pda, oxe_checkpoint_pda),
/// accrues pending yield for `staker_balance` into each checkpoint's `pending_yield`,
/// and advances `last_oxe_cumulative_yield` to the current vault accumulator.
///
/// Must be called BEFORE changing the staker's OXE balance so that the
/// yield from the previous balance is correctly captured.
///
/// # Arguments
/// * `remaining_accounts` - slice of account infos in vault+checkpoint pairs
/// * `staker_balance` - the staker's current OXE staked amount (before balance change)
/// * `owner` - the staker's pubkey (used to validate checkpoint ownership)
pub fn snapshot_oxe_checkpoints(
    remaining_accounts: &[AccountInfo],
    staker_balance: u64,
    owner: Pubkey,
) -> Result<()> {
    require!(
        remaining_accounts.len() % 2 == 0,
        OxediumError::InvalidAccountsLength
    );

    for chunk in remaining_accounts.chunks(2) {
        let vault_info = &chunk[0];
        let checkpoint_info = &chunk[1];

        // Read oxe_cumulative_yield_per_staker from vault (read-only raw bytes)
        let vault_data = vault_info.try_borrow_data()?;
        require!(
            vault_data.len() >= VAULT_OXE_CUMULATIVE_OFFSET + 16,
            OxediumError::InvalidVault
        );
        let oxe_cumulative = u128::from_le_bytes(
            vault_data[VAULT_OXE_CUMULATIVE_OFFSET..VAULT_OXE_CUMULATIVE_OFFSET + 16]
                .try_into()
                .map_err(|_| OxediumError::InvalidVault)?,
        );
        drop(vault_data);

        // Read checkpoint fields, validate, then write updated values
        {
            let mut checkpoint_data = checkpoint_info.try_borrow_mut_data()?;
            require!(
                checkpoint_data.len() >= CHECKPOINT_PENDING_OFFSET + 8,
                OxediumError::InvalidStaker
            );

            // Validate owner
            let checkpoint_owner = Pubkey::try_from(
                &checkpoint_data[CHECKPOINT_OWNER_OFFSET..CHECKPOINT_OWNER_OFFSET + 32],
            )
            .map_err(|_| OxediumError::InvalidStaker)?;
            require!(checkpoint_owner == owner, OxediumError::InvalidStaker);

            // Validate vault reference
            let checkpoint_vault = Pubkey::try_from(
                &checkpoint_data[CHECKPOINT_VAULT_OFFSET..CHECKPOINT_VAULT_OFFSET + 32],
            )
            .map_err(|_| OxediumError::InvalidVault)?;
            require!(checkpoint_vault == vault_info.key(), OxediumError::InvalidVault);

            // Read last cumulative and pending
            let last = u128::from_le_bytes(
                checkpoint_data[CHECKPOINT_LAST_OFFSET..CHECKPOINT_LAST_OFFSET + 16]
                    .try_into()
                    .map_err(|_| OxediumError::OverflowInCast)?,
            );
            let pending = u64::from_le_bytes(
                checkpoint_data[CHECKPOINT_PENDING_OFFSET..CHECKPOINT_PENDING_OFFSET + 8]
                    .try_into()
                    .map_err(|_| OxediumError::OverflowInCast)?,
            );

            // Calculate earned since last checkpoint
            let earned = calculate_staker_yield(oxe_cumulative, staker_balance, last)?;

            // Compute new pending
            let new_pending = pending
                .checked_add(earned)
                .ok_or(OxediumError::OverflowInAdd)?;

            // Write back updated values
            checkpoint_data[CHECKPOINT_LAST_OFFSET..CHECKPOINT_LAST_OFFSET + 16]
                .copy_from_slice(&oxe_cumulative.to_le_bytes());
            checkpoint_data[CHECKPOINT_PENDING_OFFSET..CHECKPOINT_PENDING_OFFSET + 8]
                .copy_from_slice(&new_pending.to_le_bytes());
        }
    }

    Ok(())
}
