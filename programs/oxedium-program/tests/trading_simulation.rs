//! End-to-end trading simulation for a SOL/USDC vault pair.
//!
//! Mirrors the on-chain instruction logic exactly to verify that fees,
//! yield accumulation, claiming and unstaking all behave as documented.
//!
//! Lifecycle simulated:
//!   Phase 1  – Staking:   Alice (100 SOL), Bob (10 SOL), Carol (18 000 USDC)
//!   Phase 2  – Swap:      Dave  1 SOL  → USDC  (balanced vaults,   base fee)
//!   Phase 3  – Swap:      Dave  5 SOL  → USDC  (slight imbalance,   base fee)
//!   Phase 4  – Swap:      Dave 10 SOL  → USDC  (growing imbalance, elevated fee)
//!   Phase 5  – Swap:      Charlie 3 600 USDC → SOL (rebalancing,   base fee)
//!   Phase 6  – Claim:     Carol collects all accrued USDC yield
//!   Phase 7  – Yield:     Alice's and Bob's SOL yield is verified
//!   Phase 8  – Unstake:   normal exit (vault healthy, no exit fee)
//!   Phase 9  – Unstake:   exit fee triggered (vault health < 50 %)
//!   Phase 10 – Collect:   admin collects protocol yield from SOL vault

use anchor_lang::prelude::Pubkey;
use oxedium_program::components::{calculate_staker_yield, compute_swap_math};
use oxedium_program::states::{Staker, Vault};
use pyth_solana_receiver_sdk::price_update::PriceFeedMessage;

// Precision scale from utils.rs
const SCALE: u128 = 1_000_000_000_000;

// ─── Oracle constants ─────────────────────────────────────────────────────────

// SOL: $180.00, Pyth exponent -8, confidence $0.10
const SOL_PRICE: i64 = 18_000_000_000;
const SOL_CONF: u64 = 10_000_000;
const SOL_EXP: i32 = -8;
const SOL_DEC: u8 = 9;

// USDC: $1.00, Pyth exponent -8, confidence $0.0001
const USDC_PRICE: i64 = 100_000_000;
const USDC_CONF: u64 = 10_000;
const USDC_EXP: i32 = -8;
const USDC_DEC: u8 = 6;

// ─── Test helpers ─────────────────────────────────────────────────────────────

fn oracle(price: i64, conf: u64, exponent: i32) -> PriceFeedMessage {
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

fn make_vault(base_fee_bps: u64, protocol_fee_bps: u64) -> Vault {
    Vault {
        base_fee_bps,
        protocol_fee_bps,
        token_mint: Pubkey::default(),
        pyth_price_account: Pubkey::default(),
        max_age_price: 0,
        initial_balance: 0,
        current_balance: 0,
        cumulative_yield_per_lp: 0,
        protocol_yield: 0,
    }
}

fn make_staker() -> Staker {
    Staker {
        owner: Pubkey::default(),
        vault: Pubkey::default(),
        staked_amount: 0,
        last_cumulative_yield: 0,
        pending_claim: 0,
    }
}

// ─── Instruction mirrors ─────────────────────────────────────────────────────

/// Mirrors staking.rs: snapshot yield, update staked amount and both vault balances.
fn do_stake(staker: &mut Staker, vault: &mut Vault, amount: u64) {
    let earned = calculate_staker_yield(
        vault.cumulative_yield_per_lp,
        staker.staked_amount,
        staker.last_cumulative_yield,
    );
    staker.pending_claim += earned;
    staker.last_cumulative_yield = vault.cumulative_yield_per_lp;
    staker.staked_amount += amount;
    vault.initial_balance += amount;
    vault.current_balance += amount;
}

/// Mirrors the state-update block in swap.rs:
///   vault_in.current  += amount_in
///   vault_out.current -= net_out
///   vault_out.cumulative_yield_per_lp += lp_fee * SCALE / initial_balance
///   vault_out.protocol_yield += protocol_fee
fn do_swap(
    vault_in: &mut Vault,
    vault_out: &mut Vault,
    amount_in: u64,
    decimals_in: u8,
    decimals_out: u8,
    oracle_in: PriceFeedMessage,
    oracle_out: PriceFeedMessage,
) -> (u64, u64, u64, u64, u64) {
    let result = compute_swap_math(
        amount_in,
        oracle_in,
        oracle_out,
        decimals_in,
        decimals_out,
        vault_in,
        vault_out,
    )
    .expect("swap math failed");

    vault_in.current_balance += amount_in;
    vault_out.current_balance -= result.net_amount_out;
    if vault_out.initial_balance > 0 {
        vault_out.cumulative_yield_per_lp +=
            (result.lp_fee_amount as u128 * SCALE) / vault_out.initial_balance as u128;
    }
    vault_out.protocol_yield += result.protocol_fee_amount;

    (
        result.swap_fee_bps,
        result.raw_amount_out,
        result.net_amount_out,
        result.lp_fee_amount,
        result.protocol_fee_amount,
    )
}

/// Mirrors claim.rs: snapshot yield into pending_claim, pay it out, reduce vault balance.
fn do_claim(staker: &mut Staker, vault: &mut Vault) -> u64 {
    let earned = calculate_staker_yield(
        vault.cumulative_yield_per_lp,
        staker.staked_amount,
        staker.last_cumulative_yield,
    );
    staker.pending_claim += earned;
    staker.last_cumulative_yield = vault.cumulative_yield_per_lp;

    let payout = staker.pending_claim;
    staker.pending_claim = 0;
    vault.current_balance -= payout;
    payout
}

/// Mirrors unstaking.rs: snapshot yield, apply exit fee when vault health < 50 %,
/// reduce staked amount and both vault balances by the full requested amount;
/// the exit fee stays in the vault and is credited to protocol_yield.
/// Returns the amount the user actually receives.
fn do_unstake(staker: &mut Staker, vault: &mut Vault, amount: u64) -> u64 {
    assert!(staker.staked_amount >= amount, "insufficient stake");

    // snapshot yield
    let earned = calculate_staker_yield(
        vault.cumulative_yield_per_lp,
        staker.staked_amount,
        staker.last_cumulative_yield,
    );
    staker.pending_claim += earned;
    staker.last_cumulative_yield = vault.cumulative_yield_per_lp;

    // dynamic exit fee (mirrors C-01 guard against division by zero)
    let liquidity_ratio = if vault.initial_balance == 0 {
        100u128
    } else {
        (vault.current_balance as u128 * 100) / vault.initial_balance as u128
    };

    let exit_fee = if liquidity_ratio < 50 {
        amount * 200 / 10_000 // 2 %
    } else {
        0
    };

    let user_receives = amount - exit_fee;

    staker.staked_amount -= amount;
    vault.initial_balance -= amount;
    vault.current_balance -= amount; // full amount leaves accounting…
    if exit_fee > 0 {
        vault.protocol_yield += exit_fee; // …but fee stays physically in ATA
    }

    user_receives
}

/// Mirrors collect.rs: transfer protocol_yield to admin, reduce vault balance.
fn do_collect(vault: &mut Vault) -> u64 {
    let amount = vault.protocol_yield;
    vault.current_balance -= amount;
    vault.protocol_yield = 0;
    amount
}

// ─── Simulation test ─────────────────────────────────────────────────────────

#[test]
fn simulate_sol_usdc_trading() {
    // ── Phase 1: Staking ────────────────────────────────────────────────────

    let mut sol_vault = make_vault(30, 5);  // base 0.30 %, protocol 0.05 %
    let mut usdc_vault = make_vault(30, 5);

    let mut alice = make_staker(); // stakes 100 SOL
    let mut bob = make_staker();   // stakes 10 SOL
    let mut carol = make_staker(); // stakes 18 000 USDC

    do_stake(&mut alice, &mut sol_vault, 100_000_000_000); // 100 SOL
    do_stake(&mut bob, &mut sol_vault, 10_000_000_000);    // 10 SOL
    do_stake(&mut carol, &mut usdc_vault, 18_000_000_000); // 18 000 USDC

    assert_eq!(sol_vault.initial_balance, 110_000_000_000);
    assert_eq!(sol_vault.current_balance, 110_000_000_000);
    assert_eq!(usdc_vault.initial_balance, 18_000_000_000);
    assert_eq!(usdc_vault.current_balance, 18_000_000_000);
    assert_eq!(alice.staked_amount, 100_000_000_000);
    assert_eq!(carol.staked_amount, 18_000_000_000);

    // ── Phase 2: Swap 1 — 1 SOL → USDC (balanced vaults) ──────────────────

    let (fee_bps, raw, net, lp, proto) = do_swap(
        &mut sol_vault,
        &mut usdc_vault,
        1_000_000_000, // 1 SOL
        SOL_DEC,
        USDC_DEC,
        oracle(SOL_PRICE, SOL_CONF, SOL_EXP),
        oracle(USDC_PRICE, USDC_CONF, USDC_EXP),
    );

    // raw_out: 1 SOL × $180 / $1 = 180 USDC = 180_000_000 micro-USDC
    assert_eq!(raw, 180_000_000);

    // imbalance fee: both vaults balanced → 30 bps
    // oracle confidence fee: SOL(10M/18B)*10000=5 bps + USDC(10K/100M)*10000=1 bps = 6 bps
    // utilization: 180_000_000 / 18_000_000_000 * 10000 = 100 bps < threshold → liquidity_fee = 30
    // adjusted = liquidity(30) + oracle(6) = 36 bps
    assert_eq!(fee_bps, 36);

    // lp_fee  = 180_000_000 × 36 / 10_000 = 648_000
    // proto   = 180_000_000 × 5  / 10_000 = 90_000
    // net_out = 180_000_000 − 648_000 − 90_000 = 179_262_000
    assert_eq!(lp, 648_000);
    assert_eq!(proto, 90_000);
    assert_eq!(net, 179_262_000);

    // SOL vault receives 1 SOL
    assert_eq!(sol_vault.current_balance, 111_000_000_000);

    // USDC vault sends only the net amount; fees stay physically in vault
    assert_eq!(usdc_vault.current_balance, 17_820_738_000);
    assert_eq!(usdc_vault.protocol_yield, 90_000);

    // Cumulative yield accumulator: 648_000 × SCALE / 18_000_000_000 = 36_000_000
    assert_eq!(usdc_vault.cumulative_yield_per_lp, 36_000_000);

    // Carol's claimable yield = 36_000 × 18_000_000_000 / SCALE = 648_000 (all lp fees)
    let carol_yield_1 =
        calculate_staker_yield(usdc_vault.cumulative_yield_per_lp, carol.staked_amount, 0);
    assert_eq!(carol_yield_1, 648_000);

    // ── Phase 3: Swap 2 — 5 SOL → USDC (slight imbalance) ─────────────────

    let (fee_bps2, raw2, net2, lp2, proto2) = do_swap(
        &mut sol_vault,
        &mut usdc_vault,
        5_000_000_000, // 5 SOL
        SOL_DEC,
        USDC_DEC,
        oracle(SOL_PRICE, SOL_CONF, SOL_EXP),
        oracle(USDC_PRICE, USDC_CONF, USDC_EXP),
    );

    // raw_out: 5 SOL × $180 = 900 USDC = 900_000_000 micro-USDC
    assert_eq!(raw2, 900_000_000);

    // Imbalance check:
    //   delta_in  = (111B − 110B) × 10_000 / 110B = 90 bps
    //   delta_out = (17_820_738_000 − 18B) × 10_000 / 18B ≈ −99 bps
    //   delta_in(90) > delta_out(−99) → quadratic curve
    //   deviation = 99 bps → curved = 99² / 10_000 = 0 (integer)
    //   imbalance_fee = base_fee = 30 bps (too small to bend curve yet)
    // utilization: 900_000_000 / 17_820_738_000 × 10_000 = 505 bps < threshold → 30 bps
    // adjusted = 30 + 6 = 36 bps (same as balanced)
    assert_eq!(fee_bps2, 36);

    // lp_fee  = 900_000_000 × 36 / 10_000 = 3_240_000
    // proto   = 900_000_000 × 5  / 10_000 = 450_000
    // net_out = 896_310_000
    assert_eq!(lp2, 3_240_000);
    assert_eq!(proto2, 450_000);
    assert_eq!(net2, 896_310_000);

    assert_eq!(sol_vault.current_balance, 116_000_000_000);
    assert_eq!(usdc_vault.current_balance, 16_924_428_000);
    assert_eq!(usdc_vault.protocol_yield, 540_000);

    // Cumulative: prev(36_000_000) + 3_240_000 × SCALE / 18B = 36_000_000 + 180_000_000 = 216_000_000
    assert_eq!(usdc_vault.cumulative_yield_per_lp, 216_000_000);

    // ── Phase 4: Swap 3 — 10 SOL → USDC (growing imbalance, elevated fee) ──

    let (fee_bps3, raw3, net3, lp3, proto3) = do_swap(
        &mut sol_vault,
        &mut usdc_vault,
        10_000_000_000, // 10 SOL
        SOL_DEC,
        USDC_DEC,
        oracle(SOL_PRICE, SOL_CONF, SOL_EXP),
        oracle(USDC_PRICE, USDC_CONF, USDC_EXP),
    );

    // raw_out: 10 × 180 = 1 800 USDC
    assert_eq!(raw3, 1_800_000_000);

    // Imbalance check:
    //   delta_in  = (116B − 110B) × 10_000 / 110B = 545 bps
    //   delta_out = (16_924_428_000 − 18B) × 10_000 / 18B ≈ −597 bps
    //   delta_in(545) > delta_out(−597) → curve
    //   deviation = 597 → curved = 597² / 10_000 = 35
    //   imbalance_fee = 30 + 9970 × 35 / 10_000 = 64 bps
    // utilization: 1_800_000_000 / 16_924_428_000 × 10_000 = 1063 bps  (just above threshold)
    //   adj = (1063 − 1000) × 10_000 / 9_000 = 70
    //   curved = 70² / 10_000 = 0 → liquidity_fee = 64 bps (curve barely starts)
    // adjusted = 64 + 6 = 70 bps
    assert_eq!(fee_bps3, 70);

    // lp_fee  = 1_800_000_000 × 70  / 10_000 = 12_600_000
    // proto   = 1_800_000_000 × 5   / 10_000 =    900_000
    // net_out = 1_786_500_000
    assert_eq!(lp3, 12_600_000);
    assert_eq!(proto3, 900_000);
    assert_eq!(net3, 1_786_500_000);

    assert_eq!(sol_vault.current_balance, 126_000_000_000);
    assert_eq!(usdc_vault.current_balance, 15_137_928_000);
    assert_eq!(usdc_vault.protocol_yield, 1_440_000);

    // Cumulative: 216_000_000 + 12_600_000 × SCALE / 18B = 216_000_000 + 700_000_000 = 916_000_000
    assert_eq!(usdc_vault.cumulative_yield_per_lp, 916_000_000);

    // ── Phase 5: Swap 4 — 3 600 USDC → SOL (rebalancing, low fee) ──────────

    let (fee_bps4, raw4, net4, lp4, proto4) = do_swap(
        &mut usdc_vault,
        &mut sol_vault,
        3_600_000_000, // 3 600 USDC
        USDC_DEC,
        SOL_DEC,
        oracle(USDC_PRICE, USDC_CONF, USDC_EXP),
        oracle(SOL_PRICE, SOL_CONF, SOL_EXP),
    );

    // raw_out: 3 600 USDC / $180 per SOL = 20 SOL = 20_000_000_000 lamports
    assert_eq!(raw4, 20_000_000_000);

    // Imbalance check — this swap rebalances:
    //   delta_in  (USDC vault) = (15_137_928_000 − 18B) × 10_000 / 18B ≈ −1_590 bps  (deficit)
    //   delta_out (SOL vault)  = (126B − 110B)           × 10_000 / 110B ≈ +1_454 bps (surplus)
    //   delta_in(−1_590) ≤ delta_out(+1_454) → rebalancing → imbalance_fee = base_fee = 30 bps ✓
    //
    // utilization: 20B / 126B × 10_000 = 1_587 bps  (above threshold)
    //   adj    = (1_587 − 1_000) × 10_000 / 9_000 = 652
    //   curved = 652² / 10_000 = 42
    //   liquidity_fee = 30 + 9970 × 42 / 10_000 = 71 bps
    // oracle conf: USDC(1) + SOL(5) = 6 bps
    // adjusted = 71 + 6 = 77 bps
    assert_eq!(fee_bps4, 77);

    // lp_fee  = 20_000_000_000 × 77 / 10_000 = 154_000_000
    // proto   = 20_000_000_000 × 5  / 10_000 =  10_000_000
    // net_out = 19_836_000_000 ≈ 19.836 SOL
    assert_eq!(lp4, 154_000_000);
    assert_eq!(proto4, 10_000_000);
    assert_eq!(net4, 19_836_000_000);

    assert_eq!(usdc_vault.current_balance, 18_737_928_000);
    assert_eq!(sol_vault.current_balance, 106_164_000_000);
    assert_eq!(sol_vault.protocol_yield, 10_000_000);

    // SOL cumulative: 154_000_000 × SCALE / 110B = 1_400_000_000
    assert_eq!(sol_vault.cumulative_yield_per_lp, 1_400_000_000);

    // Rebalancing restored USDC vault health (current > initial now)
    assert!(usdc_vault.current_balance > usdc_vault.initial_balance);

    // ── Phase 6: Carol claims all accumulated USDC yield ────────────────────

    let carol_payout = do_claim(&mut carol, &mut usdc_vault);

    // Total LP fees in USDC vault across 3 swaps: 648_000 + 3_240_000 + 12_600_000 = 16_488_000
    // Carol holds 100 % of USDC vault → she gets everything
    assert_eq!(carol_payout, 16_488_000);
    assert_eq!(carol.pending_claim, 0);
    assert_eq!(carol.last_cumulative_yield, 916_000_000);
    assert_eq!(usdc_vault.current_balance, 18_737_928_000 - 16_488_000);

    // ── Phase 7: Alice's and Bob's SOL yield proportions ────────────────────

    // Alice: 100 SOL / 110 SOL total = 100 / 110 of cumulative yield
    let alice_yield = calculate_staker_yield(
        sol_vault.cumulative_yield_per_lp,
        alice.staked_amount,
        alice.last_cumulative_yield,
    );
    // 1_400_000_000 × 100_000_000_000 / SCALE = 140_000_000 lamports = 0.14 SOL
    assert_eq!(alice_yield, 140_000_000);

    // Bob: 10 SOL / 110 SOL total
    let bob_yield = calculate_staker_yield(
        sol_vault.cumulative_yield_per_lp,
        bob.staked_amount,
        bob.last_cumulative_yield,
    );
    // 1_400_000_000 × 10_000_000_000 / SCALE = 14_000_000 lamports = 0.014 SOL
    assert_eq!(bob_yield, 14_000_000);

    // Alice + Bob == total lp_fee paid by the rebalancing swap
    assert_eq!(alice_yield + bob_yield, lp4);

    // ── Phase 8: Alice unstakes 5 SOL (vault healthy, no exit fee) ──────────

    let usdc_before_unstake = usdc_vault.current_balance; // guard no USDC side-effects

    let alice_receives = do_unstake(&mut alice, &mut sol_vault, 5_000_000_000);

    // Vault health before unstake: 106_164_000_000 / 110_000_000_000 ≈ 96 % — well above 50 %
    // → no exit fee, Alice receives her full 5 SOL
    assert_eq!(alice_receives, 5_000_000_000);
    assert_eq!(alice.staked_amount, 95_000_000_000);
    assert_eq!(sol_vault.initial_balance, 105_000_000_000);
    assert_eq!(sol_vault.current_balance, 101_164_000_000);
    assert_eq!(sol_vault.protocol_yield, 10_000_000); // unchanged
    assert_eq!(usdc_vault.current_balance, usdc_before_unstake); // USDC vault untouched

    // Pending yield was snapshotted before exit
    assert_eq!(alice.pending_claim, 140_000_000); // captured at do_unstake checkpoint

    // ── Phase 9: Exit fee triggered (vault health < 50 %) ───────────────────

    // Create an isolated USDC vault in distress and a new staker for the demo.
    let mut distressed_vault = make_vault(30, 5);
    distressed_vault.initial_balance = 18_000_000_000;
    distressed_vault.current_balance = 8_000_000_000; // 44 % health → below 50 % threshold

    let mut dave = make_staker();
    dave.staked_amount = 1_000_000_000; // Dave staked 1 000 USDC earlier

    let dave_receives = do_unstake(&mut dave, &mut distressed_vault, 1_000_000_000);

    // Exit fee = 2 % of 1_000_000_000 = 20_000_000
    // Dave receives: 1_000_000_000 − 20_000_000 = 980_000_000
    assert_eq!(dave_receives, 980_000_000);
    assert_eq!(distressed_vault.protocol_yield, 20_000_000);

    // Both balances decrease by the full requested amount (fee stays in ATA)
    assert_eq!(distressed_vault.initial_balance, 17_000_000_000);
    assert_eq!(distressed_vault.current_balance, 7_000_000_000);

    // ── Phase 10: Admin collects protocol yield from SOL vault ───────────────

    let sol_current_before_collect = sol_vault.current_balance;
    let collected = do_collect(&mut sol_vault);

    // Protocol yield was accumulated from the rebalancing swap (phase 5): 10_000_000 lamports
    assert_eq!(collected, 10_000_000);
    assert_eq!(sol_vault.protocol_yield, 0);
    assert_eq!(
        sol_vault.current_balance,
        sol_current_before_collect - 10_000_000
    );
}
