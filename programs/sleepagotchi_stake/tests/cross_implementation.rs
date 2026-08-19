//! Checks `src/lib/stake-math.ts` against this program's accrual code.
//!
//! The app has to show a staker what they have earned before the season closes,
//! and only the chain holds the denominator — so the figure is computed in
//! TypeScript from state read on chain. If the two implementations disagree, the
//! app quotes a number the program will not pay.
//!
//! The fixture is committed, so this runs on `cargo test` alone with no node
//! toolchain. Regenerate it with `pnpm stake:fixture` after touching either side;
//! a real divergence fails here.

use sleepagotchi_stake::{
    accrual::{apply_stake, apply_unstake, reward, settle},
    state::{Position, Season, SeasonParams},
};

const FIXTURE: &str = include_str!("../../../fixtures/accrual.json");

fn u64_of(value: &serde_json::Value) -> u64 {
    value.as_str().expect("expected a string").parse().unwrap()
}

fn u128_of(value: &serde_json::Value) -> u128 {
    value.as_str().expect("expected a string").parse().unwrap()
}

fn position() -> Position {
    Position {
        amount: 0,
        multiplier_bps: 10_000,
        weighted_stake_seconds: 0,
        raw_stake_seconds: 0,
        last_update_ts: 0,
        claimed: false,
        bump: 0,
    }
}

#[test]
fn agrees_with_the_typescript_implementation() {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();

    // The two constants the cap is divided by, asserted rather than assumed —
    // they are the easiest thing to drift silently.
    assert_eq!(
        fixture["secondsPerYear"].as_i64().unwrap(),
        sleepagotchi_stake::SECONDS_PER_YEAR
    );
    assert_eq!(fixture["bpsDenominator"].as_u64().unwrap(), 10_000);

    let cases = fixture["cases"].as_array().expect("cases");
    assert!(!cases.is_empty(), "fixture has no cases");

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let declared = &case["season"];

        let params = SeasonParams {
            start_ts: declared["startTs"].as_i64().unwrap(),
            end_ts: declared["endTs"].as_i64().unwrap(),
            reward_pool: u64_of(&declared["rewardPool"]),
            max_total_staked: u64_of(&declared["maxTotalStaked"]),
            max_per_wallet: u64_of(&declared["maxPerWallet"]),
            max_apr_bps: declared["maxAprBps"].as_u64().unwrap() as u16,
            max_multiplier_bps: declared["maxMultiplierBps"].as_u64().unwrap() as u32,
            // Not part of the accrual maths, so the fixture does not carry it.
            sweep_delay_seconds: sleepagotchi_stake::MIN_SWEEP_DELAY_SECONDS,
        };

        let mut season = Season {
            id: 0,
            params,
            total_staked: 0,
            total_weighted: 0,
            weighted_stake_seconds: 0,
            last_update_ts: params.start_ts,
            rewards_paid: 0,
            swept: false,
            bump: 0,
            rewards_bump: 0,
        };
        let mut positions: Vec<Position> = (0..case["positions"].as_u64().unwrap())
            .map(|_| position())
            .collect();

        for event in case["events"].as_array().unwrap() {
            let at = event["at"].as_i64().unwrap();
            let index = event["position"].as_u64().unwrap() as usize;
            let amount = u64_of(&event["amount"]);

            match event["kind"].as_str().unwrap() {
                "stake" => apply_stake(
                    &mut season,
                    &mut positions[index],
                    amount,
                    event["multiplierBps"].as_u64().unwrap() as u32,
                    at,
                )
                .unwrap_or_else(|e| panic!("{name}: stake failed: {e:?}")),
                "unstake" => apply_unstake(&mut season, &mut positions[index], amount, at)
                    .unwrap_or_else(|e| panic!("{name}: unstake failed: {e:?}")),
                other => panic!("{name}: unknown event {other}"),
            }
        }

        let settle_at = case["settleAt"].as_i64().unwrap();
        for held in positions.iter_mut() {
            settle(&mut season, held, settle_at).unwrap();
        }

        let expected = &case["expected"];
        assert_eq!(
            season.total_staked,
            u64_of(&expected["totalStaked"]),
            "{name}: total_staked"
        );
        assert_eq!(
            season.total_weighted,
            u128_of(&expected["totalWeighted"]),
            "{name}: total_weighted"
        );
        assert_eq!(
            season.weighted_stake_seconds,
            u128_of(&expected["weightedStakeSeconds"]),
            "{name}: season weighted_stake_seconds"
        );

        let declared_positions = expected["positions"].as_array().unwrap();
        assert_eq!(declared_positions.len(), positions.len(), "{name}: count");

        let mut paid: u128 = 0;
        for (index, (held, theirs)) in positions.iter().zip(declared_positions).enumerate() {
            assert_eq!(held.amount, u64_of(&theirs["amount"]), "{name}[{index}]: amount");
            assert_eq!(
                held.multiplier_bps,
                theirs["multiplierBps"].as_u64().unwrap() as u32,
                "{name}[{index}]: multiplier_bps"
            );
            assert_eq!(
                held.weighted_stake_seconds,
                u128_of(&theirs["weightedStakeSeconds"]),
                "{name}[{index}]: weighted_stake_seconds"
            );
            assert_eq!(
                held.raw_stake_seconds,
                u128_of(&theirs["rawStakeSeconds"]),
                "{name}[{index}]: raw_stake_seconds"
            );

            let owed = reward(&season, held).unwrap();
            assert_eq!(owed, u64_of(&theirs["reward"]), "{name}[{index}]: reward");
            paid += owed as u128;
        }

        // Restating the solvency property over data the app generated: whatever
        // the traffic, the shares of one pool never add up to more than it.
        assert!(
            paid <= params.reward_pool as u128,
            "{name}: paid {paid} of {}",
            params.reward_pool
        );

        // And the identity every share is divided by.
        assert_eq!(
            positions
                .iter()
                .map(|p| p.weighted_stake_seconds)
                .sum::<u128>(),
            season.weighted_stake_seconds,
            "{name}: positions do not sum to the season"
        );
    }
}

/// The fixture is only worth anything if it reaches the paths that are hard to
/// get right: both sides of the cap, and a product that overflows `u128`.
#[test]
fn the_fixture_covers_both_regimes_and_the_wide_multiply() {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    let cases = fixture["cases"].as_array().unwrap();

    let mut capped = false;
    let mut pool_bound = false;
    let mut wide = false;

    for case in cases {
        let pool = u64_of(&case["season"]["rewardPool"]) as u128;
        let paid: u128 = case["expected"]["positions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| u128_of(&p["reward"]))
            .sum();

        if pool > 0 && paid * 100 < pool {
            capped = true;
        }
        if pool > 0 && paid == pool {
            pool_bound = true;
        }

        let season_wss = u128_of(&case["expected"]["weightedStakeSeconds"]);
        if season_wss > 0 && pool.checked_mul(season_wss).is_none() {
            wide = true;
        }
    }

    assert!(capped, "no case where the APR cap leaves the pool untouched");
    assert!(pool_bound, "no case where the pool is fully spent");
    assert!(wide, "no case whose pool × stake-seconds exceeds u128");
}
