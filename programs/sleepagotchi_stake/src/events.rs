//! Every instruction that moves tokens or changes a season's economics emits one
//! of these.
//!
//! This is the program holding user principal, and its state is a time integral:
//! reconstructing why a wallet was paid what it was paid means replaying every
//! transaction that touched its position. The accrual figures travel with the
//! event so that reconstruction is a log query rather than an archaeology
//! exercise, and `SweptUnclaimed` is the one an operator should always be
//! alerted on.

use anchor_lang::prelude::*;

use crate::state::SeasonParams;

#[event]
pub struct SeasonOpened {
    pub id: u64,
    pub params: SeasonParams,
}

#[event]
pub struct SeasonUpdated {
    pub id: u64,
    pub params: SeasonParams,
}

#[event]
pub struct Staked {
    pub user: Pubkey,
    pub season: u64,
    /// This deposit, not the position total.
    pub amount: u64,
    /// Re-attested on every stake, and in force only from this instant.
    pub multiplier_bps: u32,
    pub position_amount: u64,
    pub season_total_staked: u64,
}

#[event]
pub struct Unstaked {
    pub user: Pubkey,
    pub season: u64,
    pub amount: u64,
    pub position_amount: u64,
    pub season_total_staked: u64,
}

/// Carries both integrals, so the payout can be re-derived from the log without
/// reading the accounts back.
#[event]
pub struct RewardClaimed {
    pub user: Pubkey,
    pub season: u64,
    pub owed: u64,
    pub weighted_stake_seconds: u128,
    pub raw_stake_seconds: u128,
    pub season_weighted_stake_seconds: u128,
}

#[event]
pub struct SweptUnclaimed {
    pub season: u64,
    pub amount: u64,
    pub destination: Pubkey,
}

#[event]
pub struct StakeSignerUpdated {
    pub previous: Pubkey,
    pub current: Pubkey,
}

/// Proposal only — `admin` is unchanged until the pending key signs
/// `accept_admin`. `pending` is `None` when a handover is cancelled.
#[event]
pub struct AdminTransferProposed {
    pub current: Pubkey,
    pub pending: Option<Pubkey>,
}

#[event]
pub struct AdminTransferred {
    pub previous: Pubkey,
    pub current: Pubkey,
}

#[event]
pub struct PausedSet {
    pub paused: bool,
}
