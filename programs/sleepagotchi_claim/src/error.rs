use anchor_lang::prelude::*;

/// Not `ErrorCode` — the prelude glob-imports Anchor's own enum of that name.
#[error_code]
pub enum ClaimError {
    #[msg("Claims are paused")]
    Paused,
    #[msg("Signed total is not greater than the amount already claimed")]
    NothingToClaim,
    #[msg("Vault holds less than the amount owed")]
    InsufficientVault,
    #[msg("Signer is not the pending admin, or no handover is in flight")]
    NotPendingAdmin,
}
