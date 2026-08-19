use anchor_lang::prelude::*;

use crate::error::AirdropError;

/// Singleton, seeds `[CONFIG_SEED]`. Authority on the vault, which is this
/// account's associated token account for `mint`.
#[account]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Pubkey,
    /// Proposed next admin, pending its own signature. `None` when no handover is
    /// in flight. Two steps because `admin` gates `withdraw`, which is the only
    /// non-claim path out of the vault — a key that cannot sign is a vault
    /// nobody can empty.
    pub pending_admin: Option<Pubkey>,
    pub root: [u8; 32],
    pub mint: Pubkey,
    /// Claims are live over `[start_ts, end_ts)`, unix seconds.
    pub start_ts: i64,
    pub end_ts: i64,
    /// Emergency stop, independent of the window so halting does not overwrite the
    /// announced end date.
    pub paused: bool,
    pub bump: u8,
}

/// Seeds `[RECEIPT_SEED, user]`. Empty: its existence is the claimed flag, so
/// `init` is the replay check. Never references the root, so `set_root` cannot
/// reopen a spent claim.
#[account]
#[derive(InitSpace)]
pub struct Receipt {}

pub fn validate_window(start_ts: i64, end_ts: i64) -> Result<()> {
    require!(start_ts < end_ts, AirdropError::InvalidWindow);

    Ok(())
}
