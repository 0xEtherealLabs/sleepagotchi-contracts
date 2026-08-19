use anchor_lang::prelude::*;

/// Not `ErrorCode` — the prelude glob-imports Anchor's own enum of that name.
///
/// **Append only.** `#[error_code]` numbers variants by position, so inserting
/// one renumbers every variant below it and silently changes what a client is
/// matching on. The full set is declared up front for that reason, ahead of the
/// instructions that raise them.
#[error_code]
pub enum StakeError {
    #[msg("Staking is paused")]
    Paused,

    // Season parameters and lifecycle.
    #[msg("Season start is in the past")]
    StartInThePast,
    #[msg("Season start is not before its end")]
    InvalidWindow,
    #[msg("Season parameter is zero or below its floor")]
    InvalidParameters,
    #[msg("Season has already started and is frozen")]
    SeasonStarted,
    #[msg("Season has not started yet")]
    SeasonNotStarted,
    #[msg("Season has ended")]
    SeasonEnded,
    #[msg("Season has not ended yet")]
    SeasonNotEnded,

    // Staking.
    #[msg("Amount is zero")]
    ZeroAmount,
    #[msg("Multiplier is below 1x")]
    MultiplierTooLow,
    #[msg("Multiplier is above the season maximum")]
    MultiplierTooHigh,
    #[msg("Stake would exceed the per-wallet maximum")]
    WalletCapExceeded,
    #[msg("Stake would exceed the season maximum")]
    SeasonFull,
    #[msg("Unstake is larger than the staked amount")]
    InsufficientStake,

    // Claiming and sweeping.
    #[msg("Rewards for this position have already been claimed")]
    AlreadyClaimed,
    #[msg("Season accrued no weighted stake")]
    NothingStaked,
    #[msg("Vault holds less than the amount owed")]
    InsufficientVault,
    #[msg("Season has not ended, so there is nothing to sweep")]
    SweepTooEarly,
    #[msg("Season has already been swept")]
    AlreadySwept,

    #[msg("Arithmetic overflow")]
    MathOverflow,

    #[msg("Sweep delay is below the minimum claim window")]
    SweepDelayTooShort,

    #[msg("Signer is not the pending admin, or no handover is in flight")]
    NotPendingAdmin,
}
