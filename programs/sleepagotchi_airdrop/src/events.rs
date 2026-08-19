//! Every instruction that moves tokens or changes the security posture emits
//! one of these.
//!
//! Account diffs alone are not a monitoring surface: they say what a value is
//! now, never that it changed or what it was. A root rotation and an admin
//! handover both need to be alertable the moment they land, and a claim needs to
//! reconcile against the allocation list without replaying transactions.

use anchor_lang::prelude::*;

#[event]
pub struct Claimed {
    pub user: Pubkey,
    pub amount: u64,
}

#[event]
pub struct RootUpdated {
    pub previous: [u8; 32],
    pub current: [u8; 32],
}

#[event]
pub struct WindowUpdated {
    pub start_ts: i64,
    pub end_ts: i64,
}

#[event]
pub struct PausedSet {
    pub paused: bool,
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
pub struct Withdrawn {
    pub amount: u64,
    pub destination: Pubkey,
}
