use anchor_lang::prelude::*;

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
    /// Backend key that must co-sign every claim. Never funded.
    pub claim_signer: Pubkey,
    pub mint: Pubkey,
    pub paused: bool,
    pub bump: u8,
}

/// Seeds `[RECEIPT_SEED, user]`.
#[account]
#[derive(InitSpace)]
pub struct Receipt {
    /// Running total of base units paid to this wallet. The backend signs the
    /// total, not the delta, so a resubmitted claim fails the `> claimed` check.
    pub claimed: u64,
}
