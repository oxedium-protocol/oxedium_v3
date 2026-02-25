use anchor_lang::prelude::Pubkey;
use oxedium_program::components::fees_setting;
use oxedium_program::states::Vault;

fn make_vault(base_fee_bps: u64, initial_balance: u64, current_balance: u64) -> Vault {
    Vault {
        base_fee_bps,
        protocol_fee_bps: 10,
        token_mint: Pubkey::default(),
        pyth_price_account: Pubkey::default(),
        max_age_price: 0,
        initial_balance,
        current_balance,
        cumulative_yield_per_lp: 0,
        protocol_yield: 0,
    }
}

// --- guard cases ---

#[test]
fn empty_vault_in_returns_base_fee() {
    let vault_in = make_vault(30, 0, 0);
    let vault_out = make_vault(30, 1_000_000, 1_000_000);
    assert_eq!(fees_setting(&vault_in, &vault_out), 30);
}

#[test]
fn empty_vault_out_returns_base_fee() {
    let vault_in = make_vault(30, 1_000_000, 1_000_000);
    let vault_out = make_vault(30, 0, 0);
    assert_eq!(fees_setting(&vault_in, &vault_out), 30);
}

// --- balanced vaults → base fee ---

#[test]
fn balanced_vaults_return_base_fee() {
    // Both at 100% → delta_in = 0, delta_out = 0 → delta_in <= delta_out → base fee
    let vault_in = make_vault(30, 1_000_000, 1_000_000);
    let vault_out = make_vault(30, 1_000_000, 1_000_000);
    assert_eq!(fees_setting(&vault_in, &vault_out), 30);
}

#[test]
fn swap_rebalances_pools_returns_base_fee() {
    // vault_in gained liquidity: delta_in = +10%
    // vault_out lost liquidity: delta_out = -10%
    // delta_in (1000) > delta_out (-1000) → fee increases
    //
    // But if vault_in is below initial and vault_out is above initial → delta_in <= delta_out
    // vault_in: initial=100, current=90  → delta_in  = -10% = -1000 bps
    // vault_out: initial=100, current=110 → delta_out = +10% = +1000 bps
    // delta_in (-1000) <= delta_out (1000) → base fee (swap helps rebalance)
    let vault_in = make_vault(30, 100, 90);
    let vault_out = make_vault(30, 100, 110);
    assert_eq!(fees_setting(&vault_in, &vault_out), 30);
}

// --- imbalanced vaults → elevated fee ---

#[test]
fn imbalanced_vault_out_increases_fee() {
    // vault_in: initial=100, current=150 → delta_in  = +5000 bps
    // vault_out: initial=100, current=50  → delta_out = -5000 bps
    // delta_in (5000) > delta_out (-5000) → apply curve
    //
    // deviation_bps = abs(-5000).min(10_000) = 5000
    // curved = 5000 * 5000 / 10_000 = 2_500
    // fee = 30 + (10_000 - 30) * 2_500 / 10_000
    //     = 30 + 9_970 * 2_500 / 10_000
    //     = 30 + 24_925_000 / 10_000
    //     = 30 + 2_492 = 2_522
    let vault_in = make_vault(30, 100, 150);
    let vault_out = make_vault(30, 100, 50);
    assert_eq!(fees_setting(&vault_in, &vault_out), 2_522);
}

#[test]
fn fully_drained_vault_out_near_max_fee() {
    // vault_out: initial=100, current=0 → delta_out = -10_000 bps (capped)
    // delta_in > delta_out → apply curve
    //
    // deviation_bps = min(10_000, 10_000) = 10_000
    // curved = 10_000 * 10_000 / 10_000 = 10_000
    // fee = base + (10_000 - base) * 10_000 / 10_000 = base + (10_000 - base) = 10_000
    let vault_in = make_vault(30, 100, 200);
    let vault_out = make_vault(30, 100, 0);
    assert_eq!(fees_setting(&vault_in, &vault_out), 10_000);
}

#[test]
fn small_imbalance_increases_fee_slightly() {
    // vault_in: initial=100_000, current=110_000 → delta_in  = +1000 bps
    // vault_out: initial=100_000, current=99_000  → delta_out ≈ -100 bps
    // delta_in (1000) > delta_out (-100) → apply curve
    //
    // deviation_bps = abs(-100).min(10_000) = 100
    // curved = 100 * 100 / 10_000 = 1
    // fee = 30 + (10_000 - 30) * 1 / 10_000 = 30 + 0 = 30  (integer division rounds down)
    let vault_in = make_vault(30, 100_000, 110_000);
    let vault_out = make_vault(30, 100_000, 99_000);
    // curved = 1, but (10_000 - 30) * 1 / 10_000 = 9_970 / 10_000 = 0 (floor)
    assert_eq!(fees_setting(&vault_in, &vault_out), 30);
}

#[test]
fn custom_base_fee_used_as_floor() {
    // Same imbalance as basic test but with base_fee=100 bps
    // deviation_bps = 5000, curved = 2500
    // fee = 100 + (10_000 - 100) * 2_500 / 10_000
    //     = 100 + 9_900 * 2_500 / 10_000
    //     = 100 + 2_475 = 2_575
    let vault_in = make_vault(100, 100, 150);
    let vault_out = make_vault(100, 100, 50);
    assert_eq!(fees_setting(&vault_in, &vault_out), 2_575);
}
