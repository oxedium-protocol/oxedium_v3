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

use anchor_lang::prelude::Pubkey;
use oxedium_program::components::{calculate_fee_amount, calculate_staker_yield, compute_swap_math};
use oxedium_program::states::{Staker, Vault};
use pyth_solana_receiver_sdk::price_update::PriceFeedMessage;

// Precision scale from utils.rs
const SCALE: u128 = 1_000_000_000_000;

// ─── Oracle constants ─────────────────────────────────────────────────────────

// SOL: $180.00, Pyth exponent -8
const SOL_PRICE: i64 = 18_000_000_000;
const SOL_CONF: u64 = 0;
const SOL_EXP: i32 = -8;
const SOL_DEC: u8 = 9;

// USDC: $1.00, Pyth exponent -8
const USDC_PRICE: i64 = 100_000_000;
const USDC_CONF: u64 = 0;
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
        max_exit_fee_bps: 10_000,
        token_mint: Pubkey::default(),
        pyth_price_account: Pubkey::default(),
        max_age_price: 0,
        initial_balance: 0,
        current_balance: 0,
        cumulative_yield_per_lp: 0,
        oxe_cumulative_yield_per_staker: 0,
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
    ).expect("yield calc overflow");
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
    ).expect("yield calc overflow");
    staker.pending_claim += earned;
    staker.last_cumulative_yield = vault.cumulative_yield_per_lp;

    let payout = staker.pending_claim;
    staker.pending_claim = 0;
    vault.current_balance -= payout;
    payout
}

/// Mirrors unstaking.rs exactly: snapshot yield, apply quadratic exit-fee curve,
/// reduce vault balances, redistribute exit fee into cumulative_yield_per_lp.
/// Returns the amount the user actually receives (after exit fee).
fn do_unstake(staker: &mut Staker, vault: &mut Vault, amount: u64) -> u64 {
    assert!(staker.staked_amount >= amount, "insufficient stake");

    // snapshot yield before balance changes
    let earned = calculate_staker_yield(
        vault.cumulative_yield_per_lp,
        staker.staked_amount,
        staker.last_cumulative_yield,
    ).expect("yield calc overflow");
    staker.pending_claim += earned;
    staker.last_cumulative_yield = vault.cumulative_yield_per_lp;

    // quadratic exit fee curve on health deficit (mirrors unstaking.rs)
    let health = if vault.initial_balance == 0 {
        100u128
    } else {
        (vault.current_balance as u128 * 100) / vault.initial_balance as u128
    };
    let deficit = 100u128.saturating_sub(health);
    let curved = deficit * deficit / 100;
    let exit_fee_bps = (vault.max_exit_fee_bps as u128 * curved / 100) as u64;

    let unstake_amount = if exit_fee_bps > 0 {
        calculate_fee_amount(amount, exit_fee_bps, 0)
            .expect("exit fee calc failed")
            .0
    } else {
        amount
    };

    staker.staked_amount -= amount;
    vault.initial_balance -= amount;
    vault.current_balance -= unstake_amount;

    // exit fee stays in vault, redistributed to remaining LP stakers
    let exit_fee = amount - unstake_amount;
    if exit_fee > 0 && vault.initial_balance > 0 {
        vault.cumulative_yield_per_lp +=
            (exit_fee as u128 * SCALE) / vault.initial_balance as u128;
    }

    unstake_amount
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
    // utilization: 180_000_000 / 18_000_000_000 * 10000 = 100 bps < threshold → liquidity_fee = 30
    assert_eq!(fee_bps, 30);

    // lp_fee  = 180_000_000 × 30 / 10_000 = 540_000
    // proto   = 180_000_000 × 5  / 10_000 = 90_000
    // net_out = 180_000_000 − 540_000 − 90_000 = 179_370_000
    assert_eq!(lp, 540_000);
    assert_eq!(proto, 90_000);
    assert_eq!(net, 179_370_000);

    // SOL vault receives 1 SOL
    assert_eq!(sol_vault.current_balance, 111_000_000_000);

    // USDC vault sends only the net amount; fees stay physically in vault
    assert_eq!(usdc_vault.current_balance, 17_820_630_000);

    // Cumulative yield accumulator: 540_000 × SCALE / 18_000_000_000 = 30_000_000
    assert_eq!(usdc_vault.cumulative_yield_per_lp, 30_000_000);

    // Carol's claimable yield = 30_000_000 × 18_000_000_000 / SCALE = 540_000 (all lp fees)
    let carol_yield_1 =
        calculate_staker_yield(usdc_vault.cumulative_yield_per_lp, carol.staked_amount, 0).unwrap();
    assert_eq!(carol_yield_1, 540_000);

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
    //   delta_out = (17_820_630_000 − 18B) × 10_000 / 18B ≈ −99 bps
    //   delta_in(90) > delta_out(−99) → quadratic curve
    //   deviation = 99 bps → curved = 99² / 10_000 = 0 (integer)
    //   imbalance_fee = base_fee = 30 bps (too small to bend curve yet)
    // utilization: 900_000_000 / 17_820_630_000 × 10_000 = 505 bps < threshold → 30 bps
    assert_eq!(fee_bps2, 30);

    // lp_fee  = 900_000_000 × 30 / 10_000 = 2_700_000
    // proto   = 900_000_000 × 5  / 10_000 = 450_000
    // net_out = 896_850_000
    assert_eq!(lp2, 2_700_000);
    assert_eq!(proto2, 450_000);
    assert_eq!(net2, 896_850_000);

    assert_eq!(sol_vault.current_balance, 116_000_000_000);
    assert_eq!(usdc_vault.current_balance, 16_923_780_000);

    // Cumulative: prev(30_000_000) + 2_700_000 × SCALE / 18B = 30_000_000 + 150_000_000 = 180_000_000
    assert_eq!(usdc_vault.cumulative_yield_per_lp, 180_000_000);

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
    //   delta_out = (16_923_780_000 − 18B) × 10_000 / 18B ≈ −597 bps
    //   delta_in(545) > delta_out(−597) → curve
    //   deviation = 597 → curved = 597² / 10_000 = 35
    //   imbalance_fee = 30 + 9970 × 35 / 10_000 = 64 bps
    // utilization: 1_800_000_000 / 16_923_780_000 × 10_000 = 1063 bps  (just above threshold)
    //   adj = (1063 − 1000) × 10_000 / 9_000 = 70
    //   curved = 70² / 10_000 = 0 → liquidity_fee = 64 bps (curve barely starts)
    assert_eq!(fee_bps3, 64);

    // lp_fee  = 1_800_000_000 × 64  / 10_000 = 11_520_000
    // proto   = 1_800_000_000 × 5   / 10_000 =    900_000
    // net_out = 1_787_580_000
    assert_eq!(lp3, 11_520_000);
    assert_eq!(proto3, 900_000);
    assert_eq!(net3, 1_787_580_000);

    assert_eq!(sol_vault.current_balance, 126_000_000_000);
    assert_eq!(usdc_vault.current_balance, 15_136_200_000);

    // Cumulative: 180_000_000 + 11_520_000 × SCALE / 18B = 180_000_000 + 640_000_000 = 820_000_000
    assert_eq!(usdc_vault.cumulative_yield_per_lp, 820_000_000);

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
    //   delta_in  (USDC vault) = (15_136_200_000 − 18B) × 10_000 / 18B ≈ −1_590 bps  (deficit)
    //   delta_out (SOL vault)  = (126B − 110B)           × 10_000 / 110B ≈ +1_454 bps (surplus)
    //   delta_in(−1_590) ≤ delta_out(+1_454) → rebalancing → imbalance_fee = base_fee = 30 bps ✓
    //
    // utilization: 20B / 126B × 10_000 = 1_587 bps  (above threshold)
    //   adj    = (1_587 − 1_000) × 10_000 / 9_000 = 652
    //   curved = 652² / 10_000 = 42
    //   liquidity_fee = 30 + 9970 × 42 / 10_000 = 71 bps
    assert_eq!(fee_bps4, 71);

    // lp_fee  = 20_000_000_000 × 71 / 10_000 = 142_000_000
    // proto   = 20_000_000_000 × 5  / 10_000 =  10_000_000
    // net_out = 19_848_000_000 ≈ 19.848 SOL
    assert_eq!(lp4, 142_000_000);
    assert_eq!(proto4, 10_000_000);
    assert_eq!(net4, 19_848_000_000);

    assert_eq!(usdc_vault.current_balance, 18_736_200_000);
    assert_eq!(sol_vault.current_balance, 106_152_000_000);

    // SOL cumulative: 142_000_000 × SCALE / 110B = 1_290_909_090 (floor)
    assert_eq!(sol_vault.cumulative_yield_per_lp, 1_290_909_090);

    // Rebalancing restored USDC vault health (current > initial now)
    assert!(usdc_vault.current_balance > usdc_vault.initial_balance);

    // ── Phase 6: Carol claims all accumulated USDC yield ────────────────────

    let carol_payout = do_claim(&mut carol, &mut usdc_vault);

    // Total LP fees in USDC vault across 3 swaps: 540_000 + 2_700_000 + 11_520_000 = 14_760_000
    // Carol holds 100 % of USDC vault → she gets everything
    assert_eq!(carol_payout, 14_760_000);
    assert_eq!(carol.pending_claim, 0);
    assert_eq!(carol.last_cumulative_yield, 820_000_000);
    assert_eq!(usdc_vault.current_balance, 18_736_200_000 - 14_760_000);

    // ── Phase 7: Alice's and Bob's SOL yield proportions ────────────────────

    // Alice: 100 SOL / 110 SOL total = 100 / 110 of cumulative yield
    let alice_yield = calculate_staker_yield(
        sol_vault.cumulative_yield_per_lp,
        alice.staked_amount,
        alice.last_cumulative_yield,
    ).unwrap();
    // 1_290_909_090 × 100_000_000_000 / SCALE = 129_090_909 lamports
    assert_eq!(alice_yield, 129_090_909);

    // Bob: 10 SOL / 110 SOL total
    let bob_yield = calculate_staker_yield(
        sol_vault.cumulative_yield_per_lp,
        bob.staked_amount,
        bob.last_cumulative_yield,
    ).unwrap();
    // 1_290_909_090 × 10_000_000_000 / SCALE = 12_909_090 lamports (floor)
    assert_eq!(bob_yield, 12_909_090);

    // Alice + Bob ≈ total lp_fee from the rebalancing swap (≤1 lamport rounding dust)
    assert!(lp4 - alice_yield - bob_yield <= 1);

    // ── Phase 8: Alice unstakes 5 SOL (vault healthy, no exit fee) ──────────

    let usdc_before_unstake = usdc_vault.current_balance; // guard no USDC side-effects

    let alice_receives = do_unstake(&mut alice, &mut sol_vault, 5_000_000_000);

    // Vault health before unstake: 106_152_000_000 / 110_000_000_000 ≈ 96 % — well above 50 %
    // → no exit fee, Alice receives her full 5 SOL
    assert_eq!(alice_receives, 5_000_000_000);
    assert_eq!(alice.staked_amount, 95_000_000_000);
    assert_eq!(sol_vault.initial_balance, 105_000_000_000);
    assert_eq!(sol_vault.current_balance, 101_152_000_000);
    assert_eq!(usdc_vault.current_balance, usdc_before_unstake); // USDC vault untouched

    // Pending yield was snapshotted before exit
    assert_eq!(alice.pending_claim, 129_090_909); // captured at do_unstake checkpoint

    // ── Phase 9: Exit fee triggered (vault health < 50 %) ───────────────────

    // Create an isolated USDC vault in distress and a new staker for the demo.
    let mut distressed_vault = make_vault(30, 5);
    distressed_vault.initial_balance = 18_000_000_000;
    distressed_vault.current_balance = 8_000_000_000; // 44 % health → below 50 % threshold

    let mut dave = make_staker();
    dave.staked_amount = 1_000_000_000; // Dave staked 1 000 USDC earlier

    let dave_receives = do_unstake(&mut dave, &mut distressed_vault, 1_000_000_000);

    // health = 8_000_000_000 * 100 / 18_000_000_000 = 44
    // deficit = 56, curved = 56² / 100 = 31
    // exit_fee_bps = 10_000 * 31 / 100 = 3_100 bps
    // fee = 1_000_000_000 * 3_100 / 10_000 = 310_000_000
    // Dave receives: 1_000_000_000 − 310_000_000 = 690_000_000
    assert_eq!(dave_receives, 690_000_000);

    // initial_balance decreases by full amount; current_balance by net amount only
    assert_eq!(distressed_vault.initial_balance, 17_000_000_000);
    assert_eq!(distressed_vault.current_balance, 7_310_000_000);
}
