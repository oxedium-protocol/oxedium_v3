use crate::utils::{OxediumError, SCALE};

/// Calculates the staker's yield based on cumulative yield per LP token.
///
/// # Arguments
/// * `current_cumulative_yield` - The current cumulative yield per LP token (scaled by `SCALE`)
/// * `staker_balance` - The amount of LP tokens the staker holds
/// * `last_recorded_yield` - The cumulative yield per LP token at the last update for this staker
///
/// # Returns
/// * `Result<u64, OxediumError>` - The amount of yield earned by the staker since the last update
pub fn calculate_staker_yield(
    current_cumulative_yield: u128,
    staker_balance: u64,
    last_recorded_yield: u128,
) -> Result<u64, OxediumError> {
    // Calculate the yield difference per LP token since the last update
    let delta_yield_per_lp = match current_cumulative_yield.checked_sub(last_recorded_yield) {
        Some(val) => val,
        None => return Ok(0), // If underflow occurs, return 0 yield
    };

    // Multiply the yield per LP token by the staker's LP balance to get total yield
    let total_yield = delta_yield_per_lp
        .checked_mul(staker_balance as u128)
        .ok_or(OxediumError::OverflowInMul)?;

    // Scale down the yield to normal units by dividing by SCALE
    // SCALE is a non-zero constant so checked_div can only fail if total_yield == 0,
    // which is handled by the Ok(0) path above; the ? is kept for exhaustiveness.
    let final_yield = total_yield
        .checked_div(SCALE)
        .ok_or(OxediumError::OverflowInDiv)?;

    // Saturating cast to u64: cap at u64::MAX rather than silently truncating
    Ok(final_yield.min(u64::MAX as u128) as u64)
}
