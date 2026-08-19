use anchor_lang::prelude::*;

use crate::{
    constants::{MIN_SWEEP_DELAY_SECONDS, ONE_X_BPS},
    error::StakeError,
};

/// Singleton, seeds `[CONFIG_SEED]`.
#[account]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Pubkey,
    /// Proposed next admin, pending its own signature. `None` when no handover is
    /// in flight.
    ///
    /// Less severe here than in the claim programs — `unstake` and `claim` are
    /// permissionless, so a key nobody holds strands no user funds, only
    /// `open_season` and `sweep_unclaimed`. Two steps regardless, because the
    /// three programs having one admin shape is worth more than the saving.
    pub pending_admin: Option<Pubkey>,
    /// Backend key that must co-sign every stake. Attests the multiplier, and
    /// nothing else. Never funded.
    pub stake_signer: Pubkey,
    pub mint: Pubkey,
    /// Blocks new stakes. Never blocks `unstake` or `claim`.
    pub paused: bool,
    pub next_season: u64,
    pub bump: u8,
}

/// Everything a season is configured with, as one type.
///
/// Taken whole by both `open_season` and `update_season`, which makes the freeze
/// rule a single assignment being refused rather than a per-field check a new
/// parameter could escape.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace, Debug, PartialEq, Eq)]
pub struct SeasonParams {
    pub start_ts: i64,
    pub end_ts: i64,
    /// Fixed. The reward vault holds exactly this much, from `open_season` on.
    pub reward_pool: u64,
    pub max_total_staked: u64,
    pub max_per_wallet: u64,
    pub max_apr_bps: u16,
    /// Ceiling on any attested multiplier. A bounded blast radius for a
    /// compromised or buggy signer.
    pub max_multiplier_bps: u32,
    /// How long after `end_ts` the unclaimed remainder becomes sweepable. The
    /// claim window, in other words — a sweep takes the whole vault including
    /// rewards nobody has collected yet, so this is what stops a season's
    /// payouts being cancelled the second it closes.
    pub sweep_delay_seconds: u32,
}

impl SeasonParams {
    pub fn validate(&self, now: i64) -> Result<()> {
        require!(self.start_ts >= now, StakeError::StartInThePast);
        require!(self.start_ts < self.end_ts, StakeError::InvalidWindow);
        require!(self.reward_pool > 0, StakeError::InvalidParameters);
        require!(self.max_total_staked > 0, StakeError::InvalidParameters);
        require!(self.max_per_wallet > 0, StakeError::InvalidParameters);
        require!(self.max_apr_bps > 0, StakeError::InvalidParameters);
        require!(
            self.max_multiplier_bps >= ONE_X_BPS,
            StakeError::InvalidParameters
        );
        require!(
            self.sweep_delay_seconds >= MIN_SWEEP_DELAY_SECONDS,
            StakeError::SweepDelayTooShort
        );
        Ok(())
    }

    /// The instant the remainder becomes sweepable.
    pub fn sweep_after(&self) -> Result<i64> {
        self.end_ts
            .checked_add(self.sweep_delay_seconds as i64)
            .ok_or(StakeError::MathOverflow.into())
    }
}

/// Seeds `[SEASON_SEED, id.to_le_bytes()]`. Authority on the stake vault, which
/// is this account's associated token account for the mint.
#[account]
#[derive(InitSpace)]
pub struct Season {
    pub id: u64,
    /// Rewritable in full by `update_season` until `start_ts`, frozen from that
    /// instant on. Safe because nothing can be staked before a season starts, so
    /// there is no accrual for a change to invalidate.
    pub params: SeasonParams,

    /// Raw, for the capacity check against `max_total_staked`.
    pub total_staked: u64,
    /// Σ amount × multiplier_bps over open positions, as of `last_update_ts`.
    pub total_weighted: u128,
    /// The integral of `total_weighted`. Denominator of every pro-rata share,
    /// and equal to the sum of every position's own once all are settled to
    /// `end_ts`.
    pub weighted_stake_seconds: u128,
    pub last_update_ts: i64,

    pub rewards_paid: u64,
    pub swept: bool,
    pub bump: u8,
    /// Bump for `[REWARDS_SEED, id]`, the reward vault's authority.
    pub rewards_bump: u8,
}

/// Seeds `[POSITION_SEED, season.id.to_le_bytes(), user]`. Per season, so a
/// wallet's history in one season is independent of any other.
#[account]
#[derive(InitSpace)]
pub struct Position {
    pub amount: u64,
    /// Re-attested on every stake, and applied only from that instant forward.
    pub multiplier_bps: u32,
    /// Numerator of this position's pro-rata share.
    pub weighted_stake_seconds: u128,
    /// The same integral without the multiplier. Base of the APR cap, which is
    /// a statement about tokens actually locked rather than about weight.
    pub raw_stake_seconds: u128,
    pub last_update_ts: i64,
    pub claimed: bool,
    pub bump: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn params() -> SeasonParams {
        SeasonParams {
            start_ts: NOW + 3_600,
            end_ts: NOW + 3_600 + 90 * 86_400,
            reward_pool: 2_000_000_000_000_000,
            max_total_staked: 100_000_000_000_000_000,
            max_per_wallet: 1_000_000_000_000_000,
            max_apr_bps: 1_500,
            max_multiplier_bps: 50_000,
            sweep_delay_seconds: MIN_SWEEP_DELAY_SECONDS,
        }
    }

    /// `#[error_code]` numbers variants from zero and adds `ERROR_CODE_OFFSET`
    /// on conversion, so a bare `as u32` compares against the wrong number.
    fn assert_code(result: Result<()>, expected: StakeError) {
        let Err(Error::AnchorError(err)) = result else {
            panic!("expected an anchor error, got {result:?}");
        };
        assert_eq!(
            err.error_code_number,
            expected as u32 + anchor_lang::error::ERROR_CODE_OFFSET
        );
    }

    #[test]
    fn accepts_a_well_formed_season() {
        assert!(params().validate(NOW).is_ok());
    }

    #[test]
    fn accepts_a_season_starting_immediately() {
        let p = SeasonParams {
            start_ts: NOW,
            ..params()
        };
        assert!(p.validate(NOW).is_ok());
    }

    #[test]
    fn rejects_a_start_in_the_past() {
        let p = SeasonParams {
            start_ts: NOW - 1,
            ..params()
        };
        assert_code(p.validate(NOW), StakeError::StartInThePast);
    }

    #[test]
    fn rejects_an_end_at_or_before_the_start() {
        for end_ts in [params().start_ts, params().start_ts - 1] {
            let p = SeasonParams {
                end_ts,
                ..params()
            };
            assert_code(p.validate(NOW), StakeError::InvalidWindow);
        }
    }

    #[test]
    fn rejects_a_zeroed_parameter() {
        let cases: [SeasonParams; 4] = [
            SeasonParams {
                reward_pool: 0,
                ..params()
            },
            SeasonParams {
                max_total_staked: 0,
                ..params()
            },
            SeasonParams {
                max_per_wallet: 0,
                ..params()
            },
            SeasonParams {
                max_apr_bps: 0,
                ..params()
            },
        ];
        for p in cases {
            assert_code(p.validate(NOW), StakeError::InvalidParameters);
        }
    }

    /// A multiplier below 1x would weight a position *down*, which no attested
    /// value should ever be able to do.
    #[test]
    fn rejects_a_maximum_multiplier_below_one_x() {
        let p = SeasonParams {
            max_multiplier_bps: ONE_X_BPS - 1,
            ..params()
        };
        assert_code(p.validate(NOW), StakeError::InvalidParameters);

        let exactly_one_x = SeasonParams {
            max_multiplier_bps: ONE_X_BPS,
            ..params()
        };
        assert!(exactly_one_x.validate(NOW).is_ok());
    }

    /// Without a floor there is always an interval in which the whole pool is
    /// sweepable and nothing has been claimed, because claims are impossible
    /// before `end_ts`. Zero is the value that would reintroduce it.
    #[test]
    fn rejects_a_sweep_delay_below_the_floor() {
        for sweep_delay_seconds in [0, MIN_SWEEP_DELAY_SECONDS - 1] {
            let p = SeasonParams {
                sweep_delay_seconds,
                ..params()
            };
            assert_code(p.validate(NOW), StakeError::SweepDelayTooShort);
        }

        let exactly_the_floor = SeasonParams {
            sweep_delay_seconds: MIN_SWEEP_DELAY_SECONDS,
            ..params()
        };
        assert!(exactly_the_floor.validate(NOW).is_ok());
    }

    #[test]
    fn sweep_after_is_the_close_plus_the_delay() {
        let p = params();
        assert_eq!(
            p.sweep_after().unwrap(),
            p.end_ts + MIN_SWEEP_DELAY_SECONDS as i64
        );

        let overflowing = SeasonParams {
            end_ts: i64::MAX,
            ..params()
        };
        assert_code(
            overflowing.sweep_after().map(|_| ()),
            StakeError::MathOverflow,
        );
    }
}
