use pyth_solana_receiver_sdk::price_update::PriceFeedMessage;

use crate::{
    components::{calculate_fee_amount, conf_fee_bps, fees_setting, raw_amount_out},
    states::Vault,
    utils::OxediumError,
};

pub struct SwapMathResult {
    pub swap_fee_bps: u64,
    pub raw_amount_out: u64,
    pub net_amount_out: u64,
    pub lp_fee_amount: u64,
    pub protocol_fee_amount: u64,
}

pub fn compute_swap_math(
    amount_in: u64,
    oracle_in: PriceFeedMessage,
    oracle_out: PriceFeedMessage,
    decimals_in: u8,
    decimals_out: u8,
    vault_in: &Vault,
    vault_out: &Vault
) -> Result<SwapMathResult, OxediumError> {
    let swap_fee_bps = fees_setting(&vault_in, &vault_out);

    let protocol_fee_bps = vault_out.protocol_fee_bps;

    let raw_out = raw_amount_out(amount_in, decimals_in, decimals_out, oracle_in, oracle_out)?;

    // Extra fee proportional to oracle uncertainty — protects against latency arbitrage.
    // conf/price ratio (in bps) for each oracle is summed and added on top of the swap fee.
    let oracle_fee = conf_fee_bps(oracle_in.price, oracle_in.conf, oracle_out.price, oracle_out.conf);

    // Liquidity-impact fee: flat base fee up to 10% utilization,
    // then a quadratic curve that grows aggressively from 10% to 100%.
    //
    // utilization_bps = raw_out * 10_000 / current_balance  (0..10_000)
    //
    // Below THRESHOLD (10%):
    //   liquidity_fee_bps = swap_fee_bps
    //
    // Above THRESHOLD:
    //   adj = (utilization - 10%) normalised to 0..10_000
    //   curved = adj² / 10_000
    //   liquidity_fee_bps = swap_fee_bps + (MAX_FEE - swap_fee_bps) * curved / 10_000
    //
    // Examples (swap_fee_bps = 30):
    //   10%  → 30 bps   (base only)
    //   20%  → ~148 bps
    //   50%  → ~1 997 bps (~20%)
    //   70%  → ~4 475 bps (~45%)
    //   100% → 10 000 bps (100%)
    const MAX_FEE_BPS: u64 = 10_000;
    const IMPACT_THRESHOLD_BPS: u64 = 1_000; // 10% — curve starts here

    let liquidity_fee_bps = if vault_out.current_balance == 0 {
        MAX_FEE_BPS
    } else {
        // utilization in bps, capped at 10_000
        let utilization_bps = ((raw_out as u128 * 10_000) / vault_out.current_balance as u128)
            .min(10_000) as u64;

        if utilization_bps <= IMPACT_THRESHOLD_BPS {
            swap_fee_bps
        } else {
            // shift: map 10%..100% → 0..10_000
            let adj = (utilization_bps - IMPACT_THRESHOLD_BPS) * 10_000
                / (MAX_FEE_BPS - IMPACT_THRESHOLD_BPS);

            // quadratic: adj² / 10_000  →  0..10_000
            let curved = adj * adj / 10_000;

            // scale from swap_fee_bps up to MAX_FEE_BPS
            swap_fee_bps + (MAX_FEE_BPS - swap_fee_bps) * curved / 10_000
        }
    };

    // oracle_fee is independent of the liquidity curve
    let adjusted_swap_fee_bps = liquidity_fee_bps.saturating_add(oracle_fee);

    if adjusted_swap_fee_bps + protocol_fee_bps > 10_000 {
        return Err(OxediumError::FeeExceeds.into());
    }

    let (after_fee, lp_fee, protocol_fee) =
        calculate_fee_amount(raw_out, adjusted_swap_fee_bps, protocol_fee_bps)?;

    if vault_out.current_balance < after_fee {
        return Err(OxediumError::InsufficientLiquidity.into());
    }

    Ok(SwapMathResult {
        swap_fee_bps: adjusted_swap_fee_bps,
        raw_amount_out: raw_out,
        net_amount_out: after_fee,
        lp_fee_amount: lp_fee,
        protocol_fee_amount: protocol_fee,
    })
}
