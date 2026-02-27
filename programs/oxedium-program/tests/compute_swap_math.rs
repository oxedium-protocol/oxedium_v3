use anchor_lang::prelude::Pubkey;
use oxedium_program::components::compute_swap_math;
use oxedium_program::states::Vault;
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

fn make_vault(
    base_fee_bps: u64,
    protocol_fee_bps: u64,
    initial_balance: u64,
    current_balance: u64,
) -> Vault {
    Vault {
        base_fee_bps,
        protocol_fee_bps,
        max_exit_fee_bps: 10_000,
        token_mint: Pubkey::default(),
        pyth_price_account: Pubkey::default(),
        max_age_price: 0,
        initial_balance,
        current_balance,
        cumulative_yield_per_lp: 0,
        protocol_yield: 0,
    }
}

// SOL constants (Pyth exponent = -8)
const SOL_PRICE: i64 = 10_000_000_000; // $100.00
const SOL_EXP: i32 = -8;
const SOL_DECIMALS: u8 = 9;

// USDC constants (Pyth exponent = -8)
const USDC_PRICE: i64 = 100_000_000; // $1.00
const USDC_EXP: i32 = -8;
const USDC_DECIMALS: u8 = 6;

// --- happy path ---

#[test]
fn balanced_vaults_small_swap() {
    // 10_000 lamports (0.00001 SOL) → raw_out = 1_000 micro-USDC ($0.001)
    // vaults balanced → swap_fee = 30 bps
    // conf=0 → oracle_fee = 0
    // utilization = 1_000 / 1_000_000 * 10_000 = 10 bps  ≤ 1_000 (threshold)
    //   → liquidity_fee = swap_fee = 30 bps
    // adjusted_fee = 30, protocol_fee = 10 → total = 40 ≤ 10_000 ✓
    // calculate_fee_amount(1_000, 30, 10):
    //   lp_fee = 1_000 * 30 / 10_000 = 3; max(1).min(1_000) = 3
    //   protocol_fee = 1_000 * 10 / 10_000 = 1; max(1).min(1_000) = 1
    //   net_out = 1_000 - 3 - 1 = 996
    let oracle_in = make_price_feed(SOL_PRICE, 0, SOL_EXP);
    let oracle_out = make_price_feed(USDC_PRICE, 0, USDC_EXP);
    let vault_in = make_vault(30, 0, 1_000_000, 1_000_000);
    let vault_out = make_vault(30, 10, 1_000_000, 1_000_000);

    let result = compute_swap_math(
        10_000,
        oracle_in,
        oracle_out,
        SOL_DECIMALS,
        USDC_DECIMALS,
        &vault_in,
        &vault_out,
    )
    .unwrap();

    assert_eq!(result.swap_fee_bps, 30);
    assert_eq!(result.raw_amount_out, 1_000);
    assert_eq!(result.net_amount_out, 996);
    assert_eq!(result.lp_fee_amount, 3);
    assert_eq!(result.protocol_fee_amount, 1);
}

#[test]
fn imbalanced_vault_out_elevates_fee() {
    // vault_out drained by 50% → fees_setting increases beyond base
    // 10_000 lamports SOL → raw_out = 1_000 micro-USDC
    // vault_out: initial=2_000_000, current=1_000_000 → delta_out = -5_000 bps
    // vault_in:  initial=1_000_000, current=1_100_000 → delta_in  = +1_000 bps
    // delta_in > delta_out → apply curve:
    //   deviation_bps = 5_000
    //   curved = 5_000 * 5_000 / 10_000 = 2_500
    //   swap_fee = 30 + (10_000 - 30) * 2_500 / 10_000 = 2_522
    // utilization = 1_000 / 1_000_000 * 10_000 = 10 bps → ≤ 1_000 → liquidity_fee = 2_522
    // conf = 0 → oracle_fee = 0
    // adjusted_fee = 2_522, protocol_fee = 10 → total = 2_532 ≤ 10_000 ✓
    let oracle_in = make_price_feed(SOL_PRICE, 0, SOL_EXP);
    let oracle_out = make_price_feed(USDC_PRICE, 0, USDC_EXP);
    let vault_in = make_vault(30, 0, 1_000_000, 1_100_000);
    let vault_out = make_vault(30, 10, 2_000_000, 1_000_000);

    let result = compute_swap_math(
        10_000,
        oracle_in,
        oracle_out,
        SOL_DECIMALS,
        USDC_DECIMALS,
        &vault_in,
        &vault_out,
    )
    .unwrap();

    assert_eq!(result.swap_fee_bps, 2_522);
    assert_eq!(result.raw_amount_out, 1_000);
    // lp_fee and protocol_fee are nonzero, net_out < raw_out
    assert!(result.net_amount_out < result.raw_amount_out);
}

#[test]
fn oracle_confidence_adds_to_fee() {
    // Use large conf values to generate a measurable oracle_fee
    // price_in = 100_000, conf_in = 1_000 → fee_in = 1_000*10_000/100_000 = 100 bps
    // price_out = 10_000,  conf_out = 100  → fee_out = 100*10_000/10_000  = 100 bps
    // oracle_fee = 200 bps
    //
    // Use exponent=0 for simplicity, same-value swap (price 100_000 in, 10_000 out)
    // amount_in = 1 → raw_out = 1 * 10_000 / 100_000 * (adjustment) — just check fee structure.
    //
    // Easier: use exponent=-8, price_in=100_000_000 (=$1), price_out=100_000_000 (=$1)
    //   same decimals (6→6), amount_in=100 → raw_out=100
    //   conf_in = 10_000, conf_out = 10_000
    //   fee_in  = 10_000 * 10_000 / 100_000_000 = 1 bps
    //   fee_out = 1 bps → oracle_fee = 2 bps
    // balanced vaults: swap_fee = 30, oracle_fee = 2 → adjusted = 32
    // liquidity: utilization = 100 / 1_000_000 * 10_000 = 1 bps → ≤ 1_000 → fee = 32
    let oracle_in = make_price_feed(100_000_000, 10_000, -8);
    let oracle_out = make_price_feed(100_000_000, 10_000, -8);
    let vault_in = make_vault(30, 0, 1_000_000, 1_000_000);
    let vault_out = make_vault(30, 10, 1_000_000, 1_000_000);

    let result = compute_swap_math(
        100,
        oracle_in,
        oracle_out,
        6,
        6,
        &vault_in,
        &vault_out,
    )
    .unwrap();

    assert_eq!(result.swap_fee_bps, 32); // 30 (swap) + 2 (oracle)
}

#[test]
fn high_utilization_triggers_liquidity_fee_curve() {
    // raw_out ≈ 500_000 from 1_000_000 vault → utilization = 50%
    // vault_out.current_balance = 1_000_000
    // We want raw_out to be ~500_000 (50% of vault)
    // Use same-price oracles (exponent=0, price=1), decimals 6→6
    // amount_in = 500_000 → raw_out = 500_000
    //
    // utilization_bps = 500_000 * 10_000 / 1_000_000 = 5_000 bps (50%)
    // Above IMPACT_THRESHOLD (1_000), so curve kicks in:
    //   adj = (5_000 - 1_000) * 10_000 / (10_000 - 1_000) = 4_000 * 10_000 / 9_000 = 4_444
    //   curved = 4_444 * 4_444 / 10_000 = 1_974
    //   liquidity_fee = swap_fee + (10_000 - swap_fee) * curved / 10_000
    //                 = 30 + 9_970 * 1_974 / 10_000 = 30 + 1_968 = 1_998
    let oracle = make_price_feed(1, 0, 0);
    let vault_in = make_vault(30, 0, 1_000_000, 1_000_000);
    let vault_out = make_vault(30, 0, 1_000_000, 1_000_000);

    let result = compute_swap_math(
        500_000,
        oracle.clone(),
        oracle,
        6,
        6,
        &vault_in,
        &vault_out,
    )
    .unwrap();

    // liquidity fee should be well above base_fee (30 bps)
    assert!(result.swap_fee_bps > 30);
    assert_eq!(result.raw_amount_out, 500_000);
    assert!(result.net_amount_out < 500_000);
}

// --- error cases ---

#[test]
fn insufficient_liquidity_returns_error() {
    // raw_out will exceed vault_out.current_balance
    // Use same-price oracles: 1_000_000 in → 1_000_000 out, but vault only has 100
    let oracle = make_price_feed(1, 0, 0);
    let vault_in = make_vault(30, 0, 1_000_000, 1_000_000);
    let vault_out = make_vault(30, 0, 1_000_000, 100); // only 100 units available

    let result = compute_swap_math(
        1_000_000,
        oracle.clone(),
        oracle,
        6,
        6,
        &vault_in,
        &vault_out,
    );
    assert!(result.is_err());
}

#[test]
fn fee_exceeds_100_percent_returns_error() {
    // vault_out with zero balance → liquidity_fee = 10_000 (MAX)
    // protocol_fee_bps = 10 → total = 10_010 > 10_000 → FeeExceeds
    let oracle = make_price_feed(1, 0, 0);
    let vault_in = make_vault(30, 0, 1_000_000, 1_000_000);
    let vault_out = make_vault(30, 10, 1_000_000, 0); // current_balance = 0

    let result = compute_swap_math(
        1_000,
        oracle.clone(),
        oracle,
        6,
        6,
        &vault_in,
        &vault_out,
    );
    assert!(result.is_err());
}

#[test]
fn negative_price_oracle_returns_error() {
    let oracle_bad = make_price_feed(-100, 0, -8);
    let oracle_ok = make_price_feed(USDC_PRICE, 0, USDC_EXP);
    let vault_in = make_vault(30, 0, 1_000_000, 1_000_000);
    let vault_out = make_vault(30, 10, 1_000_000, 1_000_000);

    let result = compute_swap_math(
        1_000,
        oracle_bad,
        oracle_ok,
        SOL_DECIMALS,
        USDC_DECIMALS,
        &vault_in,
        &vault_out,
    );
    assert!(result.is_err());
}
