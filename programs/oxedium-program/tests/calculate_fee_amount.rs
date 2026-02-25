use oxedium_program::components::calculate_fee_amount;

// --- zero cases ---

#[test]
fn zero_fees_returns_original_amount() {
    let (amount_out, lp_fee, protocol_fee) = calculate_fee_amount(1_000_000, 0, 0).unwrap();
    assert_eq!(amount_out, 1_000_000);
    assert_eq!(lp_fee, 0);
    assert_eq!(protocol_fee, 0);
}

#[test]
fn zero_amount_returns_zeros() {
    let (amount_out, lp_fee, protocol_fee) = calculate_fee_amount(0, 100, 50).unwrap();
    assert_eq!(amount_out, 0);
    assert_eq!(lp_fee, 0);
    assert_eq!(protocol_fee, 0);
}

// --- normal cases ---

#[test]
fn normal_fees_applied() {
    // lp_fee  = 10_000 * 30 / 10_000 = 30
    // protocol_fee = 10_000 * 10 / 10_000 = 10
    // after   = 10_000 - 30 - 10 = 9_960
    let (amount_out, lp_fee, protocol_fee) = calculate_fee_amount(10_000, 30, 10).unwrap();
    assert_eq!(lp_fee, 30);
    assert_eq!(protocol_fee, 10);
    assert_eq!(amount_out, 9_960);
}

#[test]
fn only_lp_fee() {
    // 1_000_000 * 30 / 10_000 = 3_000
    let (amount_out, lp_fee, protocol_fee) = calculate_fee_amount(1_000_000, 30, 0).unwrap();
    assert_eq!(lp_fee, 3_000);
    assert_eq!(protocol_fee, 0);
    assert_eq!(amount_out, 997_000);
}

#[test]
fn only_protocol_fee() {
    // 1_000_000 * 50 / 10_000 = 5_000
    let (amount_out, lp_fee, protocol_fee) = calculate_fee_amount(1_000_000, 0, 50).unwrap();
    assert_eq!(lp_fee, 0);
    assert_eq!(protocol_fee, 5_000);
    assert_eq!(amount_out, 995_000);
}

// --- minimum fee (1) ---

#[test]
fn minimum_fee_is_one_for_small_amount() {
    // 9_999 * 1 / 10_000 = 0 → rounded up to 1
    let (amount_out, lp_fee, _) = calculate_fee_amount(9_999, 1, 0).unwrap();
    assert_eq!(lp_fee, 1);
    assert_eq!(amount_out, 9_998);
}

#[test]
fn minimum_fee_is_one_for_amount_1() {
    // any nonzero bps with amount=1 → fee = max(0,1).min(1) = 1
    let (amount_out, lp_fee, _) = calculate_fee_amount(1, 5_000, 0).unwrap();
    assert_eq!(lp_fee, 1);
    assert_eq!(amount_out, 0);
}

// --- fee cap at 10_000 bps ---

#[test]
fn max_lp_fee_returns_zero_remainder() {
    // bps = 10_000 → fee = 1_000 * 10_000 / 10_000 = 1_000 (= amount)
    let (amount_out, lp_fee, protocol_fee) = calculate_fee_amount(1_000, 10_000, 0).unwrap();
    assert_eq!(lp_fee, 1_000);
    assert_eq!(protocol_fee, 0);
    assert_eq!(amount_out, 0);
}

// --- overflow / error cases ---

#[test]
fn fees_exceed_amount_returns_overflow_error() {
    // lp_fee = 100 * 9_000 / 10_000 = 90
    // protocol_fee = 100 * 2_000 / 10_000 = 20
    // 100 - 90 - 20 → underflow → Overflow error
    let result = calculate_fee_amount(100, 9_000, 2_000);
    assert!(result.is_err());
}

#[test]
fn large_amount_large_fees_no_overflow() {
    // u64::MAX as amount with 0 fees should be fine
    let (amount_out, lp_fee, protocol_fee) = calculate_fee_amount(u64::MAX, 0, 0).unwrap();
    assert_eq!(amount_out, u64::MAX);
    assert_eq!(lp_fee, 0);
    assert_eq!(protocol_fee, 0);
}

// --- rounding (FLOOR, not CEIL) ---

#[test]
fn fee_rounds_floor() {
    // 10_001 * 1 / 10_000 = 1 (integer division, no rounding up to 2)
    let (_, lp_fee, _) = calculate_fee_amount(10_001, 1, 0).unwrap();
    assert_eq!(lp_fee, 1);
}
