use pyth_solana_receiver_sdk::price_update::PriceFeedMessage;
use crate::utils::{SCALE, OxediumError};

pub fn raw_amount_out(
    amount_in: u64,
    decimals_in: u8,
    decimals_out: u8,
    price_message_in: PriceFeedMessage,
    price_message_out: PriceFeedMessage,
) -> Result<u64, OxediumError> {
    let amount_in = amount_in as u128;

    if price_message_in.price <= 0 || price_message_out.price <= 0 {
        return Err(OxediumError::InvalidPrice);
    }
    let price_in  = price_message_in.price  as u128;
    let price_out = price_message_out.price as u128;

    let amount_fp = amount_in
        .checked_mul(SCALE)
        .ok_or(OxediumError::OverflowInMul)?
        .checked_div(10u128.pow(decimals_in as u32))
        .ok_or(OxediumError::OverflowInDiv)?;

    let usd_fp = apply_exponent_mul(amount_fp, price_in, price_message_in.exponent)?;

    let out_fp = apply_exponent_div(usd_fp, price_out, price_message_out.exponent)?;

    let out = out_fp
        .checked_mul(10u128.pow(decimals_out as u32))
        .ok_or(OxediumError::OverflowInMul)?
        .checked_div(SCALE)
        .ok_or(OxediumError::OverflowInDiv)?;

    u64::try_from(out).map_err(|_| OxediumError::OverflowInCast)
}

/// Computes `value * price * 10^exponent`, handling the exponent sign correctly.
/// Used to convert an amount to its USD equivalent.
fn apply_exponent_mul(value: u128, price: u128, exponent: i32) -> Result<u128, OxediumError> {
    if exponent < 0 {
        let exp = exponent.unsigned_abs();
        value
            .checked_mul(price).ok_or(OxediumError::OverflowInMul)?
            .checked_div(pow10(exp)?).ok_or(OxediumError::OverflowInDiv)
    } else {
        let exp = exponent as u32;
        value
            .checked_mul(price).ok_or(OxediumError::OverflowInMul)?
            .checked_mul(pow10(exp)?).ok_or(OxediumError::OverflowInMul)
    }
}

/// Computes `value / (price * 10^exponent)`, handling the exponent sign correctly.
/// Used to convert a USD amount to the output token amount.
fn apply_exponent_div(value: u128, price: u128, exponent: i32) -> Result<u128, OxediumError> {
    if exponent < 0 {
        let exp = exponent.unsigned_abs();
        value
            .checked_mul(pow10(exp)?).ok_or(OxediumError::OverflowInMul)?
            .checked_div(price).ok_or(OxediumError::OverflowInDiv)
    } else {
        let exp = exponent as u32;
        value
            .checked_div(price).ok_or(OxediumError::OverflowInDiv)?
            .checked_div(pow10(exp)?).ok_or(OxediumError::OverflowInDiv)
    }
}

/// Returns 10^exp as u128, guarding against exponents that would overflow.
fn pow10(exp: u32) -> Result<u128, OxediumError> {
    if exp > 38 {
        return Err(OxediumError::OverflowInMul);
    }
    Ok(10u128.pow(exp))
}
