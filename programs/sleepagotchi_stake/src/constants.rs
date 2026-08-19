use anchor_lang::prelude::*;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

#[constant]
pub const SEASON_SEED: &[u8] = b"season";

/// Authority over the reward vault. Separate from the season PDA so principal
/// and rewards are different token accounts under different owners — a bug in
/// the reward maths then cannot reach anyone's stake.
#[constant]
pub const REWARDS_SEED: &[u8] = b"rewards";

#[constant]
pub const POSITION_SEED: &[u8] = b"position";

/// 365 days. Written out rather than derived so the APR denominator is readable
/// at the point of use.
#[constant]
pub const SECONDS_PER_YEAR: i64 = 31_536_000;

pub const BPS_DENOMINATOR: u64 = 10_000;

/// Floor on `sweep_delay_seconds`, seven days.
///
/// Rewards are not payable until `end_ts`, so without a floor there is always an
/// interval in which the whole pool is sweepable and nothing has been claimed —
/// one admin transaction at `end_ts + 1` would take every reward earned and not
/// yet collected. A floor rather than a fixed value: the admin still chooses how
/// long to wait, but not whether to wait at all.
#[constant]
pub const MIN_SWEEP_DELAY_SECONDS: u32 = 7 * 24 * 60 * 60;

/// The default multiplier, and the floor on any attested one. A position can be
/// weighted up, never down.
pub const ONE_X_BPS: u32 = 10_000;
