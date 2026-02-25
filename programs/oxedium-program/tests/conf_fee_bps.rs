use oxedium_program::components::conf_fee_bps;

// --- zero / guard cases ---

#[test]
fn zero_conf_both_returns_zero() {
    let fee = conf_fee_bps(100_000, 0, 100_000, 0);
    assert_eq!(fee, 0);
}

#[test]
fn zero_conf_in_returns_zero_for_that_leg() {
    // conf_in=0 → 0 bps from in-leg; conf_out gives nonzero
    // price_out=10_000, conf_out=100 → 100 * 10_000 / 10_000 = 100 bps
    let fee = conf_fee_bps(10_000, 0, 10_000, 100);
    assert_eq!(fee, 100);
}

#[test]
fn negative_price_in_returns_zero_for_that_leg() {
    // negative price → guard returns 0 for in-leg
    // price_out=10_000, conf_out=100 → 100 bps
    let fee = conf_fee_bps(-1, 500, 10_000, 100);
    assert_eq!(fee, 100);
}

#[test]
fn zero_price_in_returns_zero_for_that_leg() {
    // price <= 0 guard (price == 0)
    let fee = conf_fee_bps(0, 500, 10_000, 100);
    assert_eq!(fee, 100);
}

// --- normal cases ---

#[test]
fn symmetric_oracles_double_fee() {
    // price=10_000, conf=100 → each leg = 100 * 10_000 / 10_000 = 100 bps
    // total = 200 bps
    let fee = conf_fee_bps(10_000, 100, 10_000, 100);
    assert_eq!(fee, 200);
}

#[test]
fn asymmetric_oracles() {
    // in:  price=100_000, conf=500  → 500 * 10_000 / 100_000 = 50 bps
    // out: price=10_000,  conf=300  → 300 * 10_000 / 10_000  = 300 bps
    // total = 350 bps
    let fee = conf_fee_bps(100_000, 500, 10_000, 300);
    assert_eq!(fee, 350);
}

#[test]
fn high_confidence_ratio_low_conf_leg() {
    // SOL-like: price=$100 = 10_000_000_000 (exp -8), conf=$0.10 = 10_000_000
    // fee_in = 10_000_000 * 10_000 / 10_000_000_000 = 10 bps
    let fee = conf_fee_bps(10_000_000_000, 10_000_000, 100_000_000, 10_000);
    // USDC-like: price=$1 = 100_000_000 (exp -8), conf=$0.0001 = 10_000
    // fee_out = 10_000 * 10_000 / 100_000_000 = 1 bps
    assert_eq!(fee, 11);
}

// --- cap at 10_000 bps ---

#[test]
fn conf_larger_than_price_caps_at_10000() {
    // conf >> price → individual leg would exceed 10_000 bps each
    // each leg is min(result, 10_000), then sum is min(sum, 10_000)
    let fee = conf_fee_bps(1, 1_000_000, 1, 1_000_000);
    assert_eq!(fee, 10_000);
}

#[test]
fn one_leg_at_max_caps_total() {
    // in-leg: conf=price → 10_000 bps → already at max for that leg
    // out-leg: 0 bps
    // saturating_add → min(10_000 + 0, 10_000) = 10_000
    let fee = conf_fee_bps(1_000, 1_000, 10_000, 0);
    assert_eq!(fee, 10_000);
}

// --- precision ---

#[test]
fn small_conf_rounds_to_zero() {
    // price=10_000_000, conf=1 → 1 * 10_000 / 10_000_000 = 0 (integer division)
    let fee = conf_fee_bps(10_000_000, 1, 10_000_000, 1);
    assert_eq!(fee, 0);
}
