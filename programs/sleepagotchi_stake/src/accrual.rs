//! The accounting: settlement, the two state transitions that move stake, and
//! the reward formula.
//!
//! Policy lives in the instructions — windows, pause, capacity, multiplier
//! bounds, who signed. Nothing here reads `Clock` or touches an account. The
//! split is what lets the invariant below be tested exhaustively without a
//! validator.

use anchor_lang::prelude::*;

use crate::{
    constants::{BPS_DENOMINATOR, SECONDS_PER_YEAR},
    error::StakeError,
    math::mul_div_floor,
    state::{Position, Season},
};

impl Season {
    /// Accrual runs over `[start_ts, end_ts]` and nowhere else. Clamping here is
    /// what makes settlement idempotent after the season closes and what stops
    /// a position opened before the start accruing early.
    ///
    /// `start_ts < end_ts` is enforced by `SeasonParams::validate`, so the range
    /// is never inverted.
    fn clamp(&self, now: i64) -> i64 {
        now.clamp(self.params.start_ts, self.params.end_ts)
    }
}

fn weight(position: &Position) -> u128 {
    position.amount as u128 * position.multiplier_bps as u128
}

/// Advance both accumulators to `now`.
///
/// The only way to settle. Deliberately not two public halves: advancing a
/// position without advancing its season, or the reverse, is precisely the bug
/// that breaks `Σ position.weighted_stake_seconds == season.weighted_stake_seconds`,
/// and that identity is what the solvency argument rests on.
///
/// Must be called before any mutation of `amount`, `multiplier_bps` or
/// `total_weighted` — it integrates the values that were in force over the
/// elapsed interval, so mutating first would price the past at the new rate.
pub fn settle(season: &mut Season, position: &mut Position, now: i64) -> Result<()> {
    let t = season.clamp(now);

    let elapsed = t.saturating_sub(season.last_update_ts).max(0) as u128;
    season.weighted_stake_seconds = season
        .weighted_stake_seconds
        .checked_add(
            season
                .total_weighted
                .checked_mul(elapsed)
                .ok_or(StakeError::MathOverflow)?,
        )
        .ok_or(StakeError::MathOverflow)?;
    season.last_update_ts = t;

    // A position being opened has `amount == 0`, so this contributes nothing
    // however stale its `last_update_ts` is. Initialisation needs no special
    // case because settle-before-mutate already makes it a no-op.
    let elapsed = t.saturating_sub(position.last_update_ts).max(0) as u128;
    position.weighted_stake_seconds = position
        .weighted_stake_seconds
        .checked_add(
            weight(position)
                .checked_mul(elapsed)
                .ok_or(StakeError::MathOverflow)?,
        )
        .ok_or(StakeError::MathOverflow)?;
    position.raw_stake_seconds = position
        .raw_stake_seconds
        .checked_add(
            (position.amount as u128)
                .checked_mul(elapsed)
                .ok_or(StakeError::MathOverflow)?,
        )
        .ok_or(StakeError::MathOverflow)?;
    position.last_update_ts = t;

    Ok(())
}

/// Add to a position and re-attest its multiplier.
///
/// The new multiplier applies from this instant forward only: settlement above
/// has already banked the elapsed time at the old one, so a wallet keeps the
/// weighting it held the NFT for.
pub fn apply_stake(
    season: &mut Season,
    position: &mut Position,
    amount: u64,
    multiplier_bps: u32,
    now: i64,
) -> Result<()> {
    settle(season, position, now)?;

    season.total_weighted = season
        .total_weighted
        .checked_sub(weight(position))
        .ok_or(StakeError::MathOverflow)?;

    position.amount = position
        .amount
        .checked_add(amount)
        .ok_or(StakeError::MathOverflow)?;
    position.multiplier_bps = multiplier_bps;

    season.total_weighted = season
        .total_weighted
        .checked_add(weight(position))
        .ok_or(StakeError::MathOverflow)?;
    season.total_staked = season
        .total_staked
        .checked_add(amount)
        .ok_or(StakeError::MathOverflow)?;

    Ok(())
}

/// Remove from a position. Accrued stake-seconds are untouched — time already
/// served is already earned.
pub fn apply_unstake(
    season: &mut Season,
    position: &mut Position,
    amount: u64,
    now: i64,
) -> Result<()> {
    require!(amount <= position.amount, StakeError::InsufficientStake);

    settle(season, position, now)?;

    season.total_weighted = season
        .total_weighted
        .checked_sub(weight(position))
        .ok_or(StakeError::MathOverflow)?;

    position.amount -= amount;

    season.total_weighted = season
        .total_weighted
        .checked_add(weight(position))
        .ok_or(StakeError::MathOverflow)?;
    season.total_staked = season
        .total_staked
        .checked_sub(amount)
        .ok_or(StakeError::MathOverflow)?;

    Ok(())
}

/// What a position is owed. Only meaningful once both it and its season have
/// been settled to `end_ts`.
///
/// `min` of a share and a ceiling:
///
/// - the share is `reward_pool × position.WSS / season.WSS`, floored;
/// - the ceiling is `max_apr_bps` applied to `raw_stake_seconds`, which is the
///   multiplier-free integral, so weighting decides how the pool is divided and
///   never how much one wallet can take out of it.
///
/// The difference, where a wallet is capped, stays in the vault. Spreading it
/// over everyone else would need the set of capped wallets, which is O(n) and
/// cannot run on chain.
pub fn reward(season: &Season, position: &Position) -> Result<u64> {
    let share = if season.weighted_stake_seconds == 0 {
        0
    } else {
        mul_div_floor(
            season.params.reward_pool as u128,
            position.weighted_stake_seconds,
            season.weighted_stake_seconds,
        )
        .ok_or(StakeError::MathOverflow)?
    };

    let cap = position
        .raw_stake_seconds
        .checked_mul(season.params.max_apr_bps as u128)
        .ok_or(StakeError::MathOverflow)?
        / (BPS_DENOMINATOR as u128 * SECONDS_PER_YEAR as u128);

    // `share` never exceeds `reward_pool`, so the minimum always fits u64.
    u64::try_from(share.min(cap)).map_err(|_| StakeError::MathOverflow.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{constants::ONE_X_BPS, state::SeasonParams, test_rng::Rng};

    const START: i64 = 1_800_000_000;
    const DAY: i64 = 86_400;
    const DURATION: i64 = 90 * DAY;
    const POOL: u64 = 2_000_000_000_000_000;
    const MILLION: u64 = 1_000_000_000_000_000;

    fn season() -> Season {
        Season {
            id: 0,
            params: SeasonParams {
                start_ts: START,
                end_ts: START + DURATION,
                reward_pool: POOL,
                max_total_staked: 100 * MILLION,
                max_per_wallet: MILLION,
                max_apr_bps: 1_500,
                max_multiplier_bps: 50_000,
                sweep_delay_seconds: 0,
            },
            total_staked: 0,
            total_weighted: 0,
            weighted_stake_seconds: 0,
            last_update_ts: START,
            rewards_paid: 0,
            swept: false,
            bump: 0,
            rewards_bump: 0,
        }
    }

    fn position() -> Position {
        Position {
            amount: 0,
            multiplier_bps: ONE_X_BPS,
            weighted_stake_seconds: 0,
            raw_stake_seconds: 0,
            last_update_ts: 0,
            claimed: false,
            bump: 0,
        }
    }

    fn settle_all(season: &mut Season, positions: &mut [Position], now: i64) {
        for p in positions.iter_mut() {
            settle(season, p, now).unwrap();
        }
    }

    fn sum_weighted(positions: &[Position]) -> u128 {
        positions.iter().map(|p| p.weighted_stake_seconds).sum()
    }

    // -- the invariant ----------------------------------------------------

    /// The identity the whole design rests on, over randomised traffic:
    /// positions settle only when they are touched, the season settles on every
    /// touch by anyone, and the two integrals still agree exactly.
    #[test]
    fn season_accrual_equals_the_sum_of_every_position() {
        for seed in 0..24u64 {
            let mut rng = Rng::new(seed);
            let mut season = season();
            let mut positions = vec![position(); 12];

            let mut now = START;
            for _ in 0..400 {
                // Irregular steps, including zero — two events in one slot must
                // accrue nothing between them.
                now += match rng.below(10) {
                    0 => 0,
                    1 => rng.below(60) as i64,
                    _ => rng.below((5 * DAY) as u64) as i64,
                };

                let index = rng.below(positions.len() as u64) as usize;
                let target = &mut positions[index];

                if target.amount > 0 && rng.below(3) == 0 {
                    let amount = rng.next_u64() % (target.amount + 1);
                    apply_unstake(&mut season, target, amount, now).unwrap();
                } else {
                    let amount = rng.below(MILLION);
                    let multiplier = ONE_X_BPS + rng.below(40_001) as u32;
                    apply_stake(&mut season, target, amount, multiplier, now).unwrap();
                }
            }

            // Well past the end, so every position lands on the same clamped
            // instant however long it has been idle.
            settle_all(&mut season, &mut positions, START + DURATION + 30 * DAY);

            assert_eq!(
                sum_weighted(&positions),
                season.weighted_stake_seconds,
                "seed {seed}"
            );
            assert_eq!(
                positions.iter().map(|p| p.amount as u128).sum::<u128>(),
                season.total_staked as u128,
                "seed {seed}"
            );
            assert_eq!(
                positions.iter().map(weight).sum::<u128>(),
                season.total_weighted,
                "seed {seed}"
            );

            let paid: u128 = positions
                .iter()
                .map(|p| reward(&season, p).unwrap() as u128)
                .sum();
            assert!(paid <= POOL as u128, "seed {seed}: paid {paid} of {POOL}");
        }
    }

    // -- accrual ----------------------------------------------------------

    #[test]
    fn equal_positions_over_equal_time_split_evenly() {
        let mut season = season();
        let mut a = position();
        let mut b = position();

        apply_stake(&mut season, &mut a, MILLION, ONE_X_BPS, START).unwrap();
        apply_stake(&mut season, &mut b, MILLION, ONE_X_BPS, START).unwrap();
        settle(&mut season, &mut a, START + DURATION).unwrap();
        settle(&mut season, &mut b, START + DURATION).unwrap();
        assert_eq!(a.weighted_stake_seconds, b.weighted_stake_seconds);
        assert_eq!(
            a.weighted_stake_seconds + b.weighted_stake_seconds,
            season.weighted_stake_seconds
        );
    }

    #[test]
    fn half_the_time_is_half_the_share() {
        let mut season = season();
        let mut whole = position();
        let mut half = position();

        apply_stake(&mut season, &mut whole, MILLION, ONE_X_BPS, START).unwrap();
        apply_stake(
            &mut season,
            &mut half,
            MILLION,
            ONE_X_BPS,
            START + DURATION / 2,
        )
        .unwrap();

        settle(&mut season, &mut whole, START + DURATION).unwrap();
        settle(&mut season, &mut half, START + DURATION).unwrap();

        assert_eq!(whole.weighted_stake_seconds, half.weighted_stake_seconds * 2);
    }

    /// The multiplier is not retroactive. Ten days at 1x then eighty at 2x is
    /// not ninety days at 2x.
    #[test]
    fn a_multiplier_applies_only_from_the_instant_it_is_attested() {
        let mut season = season();
        let mut p = position();

        apply_stake(&mut season, &mut p, MILLION, ONE_X_BPS, START).unwrap();
        apply_stake(&mut season, &mut p, MILLION, 2 * ONE_X_BPS, START + 10 * DAY).unwrap();
        settle(&mut season, &mut p, START + DURATION).unwrap();

        let first = MILLION as u128 * ONE_X_BPS as u128 * (10 * DAY) as u128;
        let rest = 2 * MILLION as u128 * (2 * ONE_X_BPS) as u128 * (DURATION - 10 * DAY) as u128;
        assert_eq!(p.weighted_stake_seconds, first + rest);
    }

    #[test]
    fn a_position_opened_late_is_not_credited_for_time_before_it_existed() {
        let mut season = season();
        let mut p = position();

        apply_stake(&mut season, &mut p, MILLION, ONE_X_BPS, START + 30 * DAY).unwrap();
        settle(&mut season, &mut p, START + DURATION).unwrap();

        assert_eq!(
            p.raw_stake_seconds,
            MILLION as u128 * (DURATION - 30 * DAY) as u128
        );
    }

    /// No cooldown needed: weighting is an integral, and an interval of zero
    /// length integrates to nothing.
    #[test]
    fn staking_and_unstaking_in_one_slot_earns_nothing() {
        let mut season = season();
        let mut p = position();

        apply_stake(&mut season, &mut p, MILLION, 5 * ONE_X_BPS, START + DAY).unwrap();
        apply_unstake(&mut season, &mut p, MILLION, START + DAY).unwrap();
        settle(&mut season, &mut p, START + DURATION).unwrap();

        assert_eq!(p.weighted_stake_seconds, 0);
        assert_eq!(p.raw_stake_seconds, 0);
        assert_eq!(reward(&season, &p).unwrap(), 0);
    }

    #[test]
    fn accrual_stops_at_the_end_and_settling_again_changes_nothing() {
        let mut season = season();
        let mut p = position();

        apply_stake(&mut season, &mut p, MILLION, ONE_X_BPS, START).unwrap();
        settle(&mut season, &mut p, START + DURATION).unwrap();
        let at_end = p.weighted_stake_seconds;

        settle(&mut season, &mut p, START + DURATION + 365 * DAY).unwrap();
        assert_eq!(p.weighted_stake_seconds, at_end);
        assert_eq!(season.weighted_stake_seconds, at_end);
    }

    /// The lower clamp matters for a reason the upper one does not: a settle
    /// stamped before the window would drag `last_update_ts` backwards, and the
    /// next settle would then bill the same interval twice.
    #[test]
    fn settling_before_the_start_neither_accrues_nor_rewinds_the_clock() {
        let mut season = season();
        let mut p = position();

        apply_stake(&mut season, &mut p, MILLION, ONE_X_BPS, START).unwrap();

        settle(&mut season, &mut p, START - 30 * DAY).unwrap();
        assert_eq!(season.last_update_ts, START);
        assert_eq!(p.last_update_ts, START);
        assert_eq!(p.weighted_stake_seconds, 0);

        settle(&mut season, &mut p, START + DURATION).unwrap();
        assert_eq!(
            p.weighted_stake_seconds,
            MILLION as u128 * ONE_X_BPS as u128 * DURATION as u128
        );
        assert_eq!(season.weighted_stake_seconds, p.weighted_stake_seconds);
    }

    /// Staking a large amount a second before the close buys a second of
    /// weight, so there is no end-of-season snapshot to front-run.
    #[test]
    fn a_late_stake_earns_only_the_time_it_was_present_for() {
        let mut season = season();
        let mut early = position();
        let mut late = position();

        apply_stake(&mut season, &mut early, MILLION, ONE_X_BPS, START).unwrap();
        apply_stake(&mut season, &mut late, 50 * MILLION, ONE_X_BPS, START + DURATION - 1).unwrap();

        settle(&mut season, &mut early, START + DURATION).unwrap();
        settle(&mut season, &mut late, START + DURATION).unwrap();

        assert!(late.weighted_stake_seconds < early.weighted_stake_seconds / 7_000);
    }

    // -- the cap ----------------------------------------------------------

    /// A sole staker's pro-rata share is the entire pool. The cap is what they
    /// actually receive, and the rest of the pool goes unclaimed.
    #[test]
    fn a_sole_staker_receives_the_cap_and_not_the_pool() {
        let mut season = season();
        let mut p = position();

        apply_stake(&mut season, &mut p, MILLION, ONE_X_BPS, START).unwrap();
        settle(&mut season, &mut p, START + DURATION).unwrap();

        let expected = 1_500u128 * MILLION as u128 * DURATION as u128
            / (BPS_DENOMINATOR as u128 * SECONDS_PER_YEAR as u128);
        assert_eq!(reward(&season, &p).unwrap() as u128, expected);
        assert_eq!(expected, 36_986_301_369_863);

        // 15% of a year, pro-rated to ninety days, and far below the pool.
        assert!(expected < POOL as u128 / 50);
    }

    /// The multiplier moves share, never the ceiling. Both wallets hold the same
    /// tokens for the same time, so both cap identically however they are
    /// weighted — even though the 4x wallet's raw pro-rata share is four times
    /// the other's.
    #[test]
    fn the_multiplier_does_not_lift_the_cap() {
        let mut season = season();
        let mut plain = position();
        let mut boosted = position();

        apply_stake(&mut season, &mut plain, MILLION, ONE_X_BPS, START).unwrap();
        apply_stake(&mut season, &mut boosted, MILLION, 4 * ONE_X_BPS, START).unwrap();
        settle(&mut season, &mut plain, START + DURATION).unwrap();
        settle(&mut season, &mut boosted, START + DURATION).unwrap();

        assert_eq!(
            boosted.weighted_stake_seconds,
            plain.weighted_stake_seconds * 4
        );
        assert_eq!(boosted.raw_stake_seconds, plain.raw_stake_seconds);
        assert_eq!(
            reward(&season, &boosted).unwrap(),
            reward(&season, &plain).unwrap()
        );
    }

    /// The cap is applied to stake-seconds, so emptying a position before the
    /// close does not shrink it. A cap on the closing balance would pay this
    /// wallet nothing.
    #[test]
    fn unstaking_before_the_close_does_not_shrink_the_cap() {
        let mut season = season();
        let mut p = position();
        let mut other = position();

        // Someone else large enough that pro-rata, not the cap, would bind.
        apply_stake(&mut season, &mut other, 50 * MILLION, ONE_X_BPS, START).unwrap();

        apply_stake(&mut season, &mut p, MILLION, ONE_X_BPS, START).unwrap();
        apply_unstake(&mut season, &mut p, MILLION - 1, START + DURATION - 1).unwrap();
        settle(&mut season, &mut p, START + DURATION).unwrap();

        assert_eq!(p.amount, 1);
        assert!(reward(&season, &p).unwrap() > 0);
        assert_eq!(
            p.raw_stake_seconds,
            MILLION as u128 * (DURATION - 1) as u128 + 1
        );
    }

    /// Cycling cannot manufacture stake-seconds: the ceiling on `raw_stake_seconds`
    /// is holding the per-wallet maximum for the whole season, however it is reached.
    #[test]
    fn cycling_cannot_exceed_the_ceiling() {
        let mut season = season();
        let mut p = position();
        let mut rng = Rng::new(0xCE11);

        let mut now = START;
        while now < START + DURATION {
            now += rng.below((2 * DAY) as u64) as i64;
            let held = p.amount;
            if held > 0 && rng.below(2) == 0 {
                apply_unstake(&mut season, &mut p, held, now).unwrap();
            } else {
                let room = season.params.max_per_wallet - held;
                apply_stake(&mut season, &mut p, rng.next_u64() % (room + 1), ONE_X_BPS, now)
                    .unwrap();
            }
        }
        settle(&mut season, &mut p, START + DURATION).unwrap();

        let ceiling = season.params.max_per_wallet as u128 * DURATION as u128;
        assert!(p.raw_stake_seconds <= ceiling);
    }

    #[test]
    fn a_season_nobody_staked_in_pays_nothing() {
        let season = season();
        let p = position();
        assert_eq!(season.weighted_stake_seconds, 0);
        assert_eq!(reward(&season, &p).unwrap(), 0);
    }

    #[test]
    fn refuses_an_unstake_larger_than_the_position() {
        let mut season = season();
        let mut p = position();

        apply_stake(&mut season, &mut p, MILLION, ONE_X_BPS, START).unwrap();
        assert!(apply_unstake(&mut season, &mut p, MILLION + 1, START + DAY).is_err());

        // And left the position untouched.
        assert_eq!(p.amount, MILLION);
        assert_eq!(season.total_staked, MILLION);
    }
}
