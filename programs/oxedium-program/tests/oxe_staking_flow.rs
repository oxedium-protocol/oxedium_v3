// // Integration test for OXE staking flow
// // Tests the complete cycle: stake → swap → claim → unstake

// #[cfg(test)]
// mod oxe_staking_tests {
//     use oxedium_program::components::calculate_staker_yield;
//     use oxedium_program::utils::SCALE;
//     use oxedium_program::states::{Vault, OxeCheckpoint, OxeStaker};

//     /// Test case scenario:
//     /// 1. User A stakes 1000 OXE
//     /// 2. Swap happens in vault_A: +100 protocol fees
//     /// 3. Cumulative yield per staker = (100 * SCALE) / 1000 = 0.1 * SCALE
//     /// 4. User A's earned = 0.1 * SCALE * 1000 / SCALE = 100
//     /// 5. User A claims 100 tokens
//     #[test]
//     fn oxe_stake_single_vault_single_swap() {
//         // Simulate: oxe_stake with amount=1000
//         let staked_amount = 1000u64;
//         let old_accumulator = 0u128;
//         let new_accumulator = (100u128 * SCALE) / 1000u128; // protocol_fee / total_staked

//         // After swap, checkpoint should have calculated earned
//         let earned = calculate_staker_yield(new_accumulator, staked_amount, old_accumulator).unwrap();
        
//         assert_eq!(earned, 100, "User should earn 100 tokens from protocol fees");
//     }

//     /// Test case: OXE staker adds more OXE mid-earn cycle
//     /// 1. User A stakes 1000 OXE, checkpoint at accumulator=0
//     /// 2. Swap: accumulator becomes (100*SCALE)/1000 = 0.1*SCALE
//     /// 3. User A stakes additional 1000 OXE (now has 2000):
//     ///    - Should accrue earned=100 at old balance (1000)
//     ///    - Checkpoint updates to 0.1*SCALE
//     /// 4. Another swap adds 100 more protocol fees: accumulator becomes 0.1*SCALE + (100*SCALE)/2000 = 0.15*SCALE
//     /// 5. User A claims:
//     ///    - pending_yield = 100 (from step 3)
//     ///    - earned_since = (0.15 - 0.1) * 2000 / SCALE = 100
//     ///    - total = 200
//     #[test]
//     fn oxe_stake_increase_mid_cycle() {
//         // Initial stake and first swap
//         let first_accumulator = (100u128 * SCALE) / 1000u128;
//         let earned_first = calculate_staker_yield(first_accumulator, 1000, 0).unwrap();
//         assert_eq!(earned_first, 100);

//         // Increase stake: accumulated yield at old balance
//         let accumulated_so_far = earned_first;

//         // Second swap with 2000 OXE staked
//         // Additional 100 protocol fees added to pool with 2000 OXE
//         let second_accumulator = first_accumulator + (100u128 * SCALE) / 2000u128;
//         let earned_second = calculate_staker_yield(second_accumulator, 2000, first_accumulator).unwrap();
        
//         let total_earned = accumulated_so_far + earned_second;
//         assert_eq!(total_earned, 200, "Total earned should account for both periods");
//     }

//     /// Test case: Multiple vaults with different yields
//     /// This tests that snapshots correctly handle multiple vaults
//     #[test]
//     fn oxe_stake_multiple_vaults() {
//         // Vault A: 10 tokens protocol fee, 1000 total staked → 0.01 per OXE
//         let vault_a_acc = (10u128 * SCALE) / 1000u128;
        
//         // Vault B: 50 tokens protocol fee, 1000 total staked → 0.05 per OXE
//         let vault_b_acc = (50u128 * SCALE) / 1000u128;

//         let staked = 1000u64;
//         let earned_a = calculate_staker_yield(vault_a_acc, staked, 0).unwrap();
//         let earned_b = calculate_staker_yield(vault_b_acc, staked, 0).unwrap();

//         assert_eq!(earned_a, 10, "Vault A should yield 10");
//         assert_eq!(earned_b, 50, "Vault B should yield 50");
//     }

//     /// Test edge case: Zero total_staked (division safety)
//     /// When no one is staked, protocol fees should not cause errors
//     #[test]
//     fn oxe_stake_zero_total_staked() {
//         // If total_staked is 0, swap should skip the OXE yield update
//         // This is handled in swap.rs: `if total_staked > 0 { ... }`
//         // calculate_staker_yield should return 0 for any balance if no change in accumulator
//         let result = calculate_staker_yield(0, 1000, 0).unwrap();
//         assert_eq!(result, 0, "No accumulator change means no yield");
//     }

//     /// Test fractional yield calculation (precision with SCALE)
//     #[test]
//     fn oxe_stake_fractional_yields() {
//         // Scenario: 5 protocol fees from 10,000 total staked
//         // Yield per OXE = 5/10,000 = 0.0005
//         // For 100 OXE: earned = 0.0005 * 100 = 0.05
//         let accumulator = (5u128 * SCALE) / 10_000u128;
//         let earned = calculate_staker_yield(accumulator, 100, 0).unwrap();
        
//         // With SCALE = 10^15, this should be very precise
//         let expected = (5u128 * 100 * SCALE / 10_000 / SCALE) as u64;
//         assert_eq!(earned, expected);
//     }

//     /// Test that snapshot_oxe_checkpoints correctly isolates per-vault earnings
//     /// When user unstakes from vault A, it shouldn't affect vault B earnings
//     #[test]
//     fn oxe_unstake_snapshot_isolation() {
//         // User staked in both vault A and B
//         let vault_a_acc = (100u128 * SCALE) / 1000u128;
//         let vault_b_acc = (200u128 * SCALE) / 1000u128;
//         let staked = 1000u64;

//         // Checkpoints start at 0 for both
//         let pending_a = 0u64;
//         let pending_b = 0u64;

//         // Calculate earnings at old balance before unstaking
//         let earned_a = calculate_staker_yield(vault_a_acc, staked, 0).unwrap();
//         let earned_b = calculate_staker_yield(vault_b_acc, staked, 0).unwrap();

//         // snapshot_oxe_checkpoints would update both independently
//         let new_pending_a = pending_a + earned_a;
//         let new_pending_b = pending_b + earned_b;

//         assert_eq!(new_pending_a, 100, "Vault A pending should be 100");
//         assert_eq!(new_pending_b, 200, "Vault B pending should be 200");
//         // After this, balances change but checkpoint values are preserved
//     }

//     /// New instruction test: sync single vault checkpoint
//     #[test]
//     fn sync_checkpoint_simple() {
//         // simulate a vault with some protocol fees added
//         let staked_amount = 500u64;
//         let initial_acc = 0u128;
//         let vault_acc = (25u128 * SCALE) / 500u128; // 25 fees, 500 staked

//         // first call: checkpoint created, no pending
//         let mut owner = Pubkey::default();
//         let mut vault = Vault {
//             base_fee_bps: 0,
//             protocol_fee_bps: 0,
//             max_exit_fee_bps: 0,
//             token_mint: Pubkey::default(),
//             pyth_price_account: Pubkey::default(),
//             max_age_price: 0,
//             initial_balance: 0,
//             current_balance: 0,
//             cumulative_yield_per_lp: 0,
//             oxe_cumulative_yield_per_staker: vault_acc,
//         };
//         let mut checkpoint: OxeCheckpoint = OxeCheckpoint { owner: Pubkey::default(), vault: Pubkey::default(), last_oxe_cumulative_yield: 0, pending_yield: 0 };
//         let mut staker = OxeStaker { owner: Pubkey::default(), staked_amount };

//         // mimic logic from sync_oxe_checkpoint
//         if checkpoint.owner == Pubkey::default() {
//             checkpoint.owner = owner;
//             checkpoint.vault = vault.key();
//             checkpoint.last_oxe_cumulative_yield = vault.oxe_cumulative_yield_per_staker;
//             checkpoint.pending_yield = 0;
//         }
//         assert_eq!(checkpoint.last_oxe_cumulative_yield, vault_acc);
//         assert_eq!(checkpoint.pending_yield, 0);

//         // simulate another swap increasing accumulator
//         let new_vault_acc = vault_acc + (75u128 * SCALE) / 500u128;
//         vault.oxe_cumulative_yield_per_staker = new_vault_acc;

//         // call sync again: should accrue yield for old balance
//         let earned = calculate_staker_yield(
//             vault.oxe_cumulative_yield_per_staker,
//             staker.staked_amount,
//             checkpoint.last_oxe_cumulative_yield,
//         ).unwrap();
//         checkpoint.pending_yield = checkpoint.pending_yield.checked_add(earned).unwrap();
//         checkpoint.last_oxe_cumulative_yield = vault.oxe_cumulative_yield_per_staker;

//         assert_eq!(earned, 75);
//         assert_eq!(checkpoint.pending_yield, 75);
//     }
// }
