//! Every instruction that moves tokens or changes the security posture emits
//! one of these.
//!
//! The detection story for a leaked `claim_signer` is otherwise "notice the
//! vault balance moved". `Claimed` carries both the delta and the new running
//! total, so a listener can reconcile against the backend's entitlement ledger
//! without replaying transactions, and `ClaimSignerUpdated` makes a rotation
//! alertable the moment it lands.

use anchor_lang::prelude::*;

#[event]
pub struct Claimed {
    pub user: Pubkey,
    /// What this transaction paid — `total` minus whatever the receipt already held.
    pub owed: u64,
    /// The receipt's new running total, which is what the signer attested.
    pub total: u64,
}

#[event]
pub struct ClaimSignerUpdated {
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

#[event]
pub struct Withdrawn {
    pub amount: u64,
    pub destination: Pubkey,
}
