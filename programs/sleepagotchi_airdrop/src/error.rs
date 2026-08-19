use anchor_lang::prelude::*;

/// Not `ErrorCode` — the prelude glob-imports Anchor's own enum of that name.
#[error_code]
pub enum AirdropError {
    #[msg("Claims are paused")]
    Paused,
    #[msg("The claim window has not opened")]
    NotStarted,
    #[msg("The claim window has closed")]
    Ended,
    #[msg("Claim window start must be before its end")]
    InvalidWindow,
    #[msg("Merkle proof does not prove this allocation")]
    InvalidProof,
    #[msg("Vault holds less than the allocation")]
    InsufficientVault,
    #[msg("Signer is not the pending admin, or no handover is in flight")]
    NotPendingAdmin,
}
