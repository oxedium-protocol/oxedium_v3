use oxedium_program::components::raw_amount_out;
use pyth_solana_receiver_sdk::price_update::PriceFeedMessage;

fn make_price_feed(price: i64, conf: u64, exponent: i32) -> PriceFeedMessage {
    PriceFeedMessage {
        feed_id: [0u8; 32],
        price,
        conf,
        exponent,
        publish_time: 1_700_000_000,
        prev_publish_time: 1_699_999_999,
        ema_price: price,
        ema_conf: conf,
    }
}

// SOL oracle: price = $100, Pyth exponent = -8
//   → stored price = 10_000_000_000, exponent = -8
//   → real price = 10_000_000_000 * 10^(-8) = $100.00
const SOL_PRICE: i64 = 10_000_000_000;
const SOL_EXP: i32 = -8;
const SOL_DECIMALS: u8 = 9;

// USDC oracle: price = $1, Pyth exponent = -8
//   → stored price = 100_000_000, exponent = -8
//   → real price = 100_000_000 * 10^(-8) = $1.00
const USDC_PRICE: i64 = 100_000_000;
const USDC_EXP: i32 = -8;
const USDC_DECIMALS: u8 = 6;

// --- SOL → USDC ---

#[test]
fn one_sol_gives_100_usdc() {
    // 1 SOL = 1_000_000_000 lamports
    // expected: 100 USDC = 100_000_000 micro-USDC
    let oracle_in = make_price_feed(SOL_PRICE, 0, SOL_EXP);
    let oracle_out = make_price_feed(USDC_PRICE, 0, USDC_EXP);

    let out = raw_amount_out(1_000_000_000, SOL_DECIMALS, USDC_DECIMALS, oracle_in, oracle_out).unwrap();
    assert_eq!(out, 100_000_000); // 100 USDC
}

#[test]
fn small_sol_amount() {
    // 10_000 lamports (0.00001 SOL) → expected 0.001 USDC = 1_000 micro-USDC
    let oracle_in = make_price_feed(SOL_PRICE, 0, SOL_EXP);
    let oracle_out = make_price_feed(USDC_PRICE, 0, USDC_EXP);

    let out = raw_amount_out(10_000, SOL_DECIMALS, USDC_DECIMALS, oracle_in, oracle_out).unwrap();
    assert_eq!(out, 1_000); // 0.001 USDC
}

// --- USDC → SOL ---

#[test]
fn hundred_usdc_gives_one_sol() {
    // 100 USDC = 100_000_000 micro-USDC
    // expected: 1 SOL = 1_000_000_000 lamports
    let oracle_in = make_price_feed(USDC_PRICE, 0, USDC_EXP);
    let oracle_out = make_price_feed(SOL_PRICE, 0, SOL_EXP);

    let out = raw_amount_out(100_000_000, USDC_DECIMALS, SOL_DECIMALS, oracle_in, oracle_out).unwrap();
    assert_eq!(out, 1_000_000_000); // 1 SOL
}

#[test]
fn one_usdc_gives_correct_sol_fraction() {
    // 1 USDC = 1_000_000 micro-USDC → 0.01 SOL = 10_000_000 lamports
    let oracle_in = make_price_feed(USDC_PRICE, 0, USDC_EXP);
    let oracle_out = make_price_feed(SOL_PRICE, 0, SOL_EXP);

    let out = raw_amount_out(1_000_000, USDC_DECIMALS, SOL_DECIMALS, oracle_in, oracle_out).unwrap();
    assert_eq!(out, 10_000_000); // 0.01 SOL
}

// --- same token swap (identity) ---

#[test]
fn same_price_and_decimals_returns_same_amount() {
    // Both tokens at $100 with 6 decimals → 1:1 swap
    let oracle = make_price_feed(10_000_000_000, 0, -8);
    let out = raw_amount_out(500_000, 6, 6, oracle.clone(), oracle).unwrap();
    assert_eq!(out, 500_000);
}

// --- non-negative exponents ---

#[test]
fn positive_exponent_handled_correctly() {
    // exponent = 0 means price is exact (no scaling)
    // price_in = 100 (= $100, no scaling), price_out = 1 (= $1, no scaling)
    // amount_in = 1_000_000 (with decimals_in = 6)
    // expected: 100_000_000 (with decimals_out = 6) — 100× more output units

    // Step trace:
    // amount_fp = 1_000_000 * SCALE / 10^6 = SCALE
    // usd_fp    = SCALE * 100 * 1         = 100 * SCALE  (positive exp path: value * price * 10^0)
    // out_fp    = 100 * SCALE / 1 / 1     = 100 * SCALE  (positive exp path: value / price / 10^0)
    // out       = 100 * SCALE * 10^6 / SCALE = 100_000_000
    let oracle_in = make_price_feed(100, 0, 0);
    let oracle_out = make_price_feed(1, 0, 0);
    let out = raw_amount_out(1_000_000, 6, 6, oracle_in, oracle_out).unwrap();
    assert_eq!(out, 100_000_000);
}

// --- error cases ---

#[test]
fn negative_price_in_returns_error() {
    let oracle_in = make_price_feed(-1, 0, -8);
    let oracle_out = make_price_feed(USDC_PRICE, 0, USDC_EXP);

    let result = raw_amount_out(1_000_000_000, SOL_DECIMALS, USDC_DECIMALS, oracle_in, oracle_out);
    assert!(result.is_err());
}

#[test]
fn zero_price_in_returns_error() {
    let oracle_in = make_price_feed(0, 0, -8);
    let oracle_out = make_price_feed(USDC_PRICE, 0, USDC_EXP);

    let result = raw_amount_out(1_000_000_000, SOL_DECIMALS, USDC_DECIMALS, oracle_in, oracle_out);
    assert!(result.is_err());
}

#[test]
fn negative_price_out_returns_error() {
    let oracle_in = make_price_feed(SOL_PRICE, 0, SOL_EXP);
    let oracle_out = make_price_feed(-5, 0, -8);

    let result = raw_amount_out(1_000_000_000, SOL_DECIMALS, USDC_DECIMALS, oracle_in, oracle_out);
    assert!(result.is_err());
}

#[test]
fn exponent_too_large_returns_overflow_error() {
    // exponent > 38 triggers pow10 overflow guard
    let oracle_in = make_price_feed(100, 0, 39);
    let oracle_out = make_price_feed(100, 0, -8);

    let result = raw_amount_out(1_000_000, 6, 6, oracle_in, oracle_out);
    assert!(result.is_err());
}

// --- zero amount ---

#[test]
fn zero_amount_in_returns_zero() {
    let oracle_in = make_price_feed(SOL_PRICE, 0, SOL_EXP);
    let oracle_out = make_price_feed(USDC_PRICE, 0, USDC_EXP);

    let out = raw_amount_out(0, SOL_DECIMALS, USDC_DECIMALS, oracle_in, oracle_out).unwrap();
    assert_eq!(out, 0);
}
