use crate::utils::OxediumError;

/// Calculates the resulting amount after applying LP, protocol, and partner fees.
/// 
/// # Arguments
/// * `amount` - The initial amount to apply fees on
/// * `lp_fee_bps` - LP fee in basis points (bps, 1 bps = 0.01%) applied to the full amount
/// * `protocol_fee_bps` - Protocol fee in bps applied to the full amount
///
/// # Returns
/// * `Result<(amount_after_fee, lp_fee, protocol_fee), TyrbineError>` - 
///   Tuple containing the remaining amount after all fees and each individual fee amount
pub fn calculate_fee_amount(
    amount: u64,
    lp_fee_bps: u64,
    protocol_fee_bps: u64,
) -> Result<(u64, u64, u64), OxediumError> {

    // Calculate LP fee from the original amount
    let lp_fee = fee(amount, lp_fee_bps)?;

    // Calculate protocol fee as a percentage of LP fee
    let protocol_fee = fee(amount, protocol_fee_bps)?;
    
    // Subtract LP fee, protocol fee, fee sequentially from the original amount
    let amount_after_fee = amount
        .checked_sub(lp_fee)
        .and_then(|v| v.checked_sub(protocol_fee))
        .ok_or(OxediumError::Overflow)?;

    // Return the remaining amount and all individual fees
    Ok((amount_after_fee, lp_fee, protocol_fee))
}

/// Helper function to calculate fee in basis points (bps) with ceiling rounding.
///
/// Uses `ceil(amount * bps / 10_000)` to prevent fee evasion via dust amounts
/// while remaining proportional for all meaningful values.
fn fee(amount: u64, bps: u64) -> Result<u64, OxediumError> {
    if bps == 0 || amount == 0 {
        return Ok(0);
    }
    // ceil(amount * bps / 10_000) via (amount * bps + 9_999) / 10_000.
    // amount <= u64::MAX, bps <= 10_000, so amount*bps+9_999 fits in u128.
    let f = ((amount as u128 * bps as u128 + 9_999) / 10_000) as u64;
    Ok(f.min(amount))
}

