pub mod accrual;
pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod math;
pub mod state;

#[cfg(test)]
mod test_rng;

use anchor_lang::prelude::*;

pub use constants::*;
pub use events::*;
pub use instructions::*;
pub use state::*;

declare_id!("AbXC8pN6zbyoi3qWxLcMQC3yXhNUHmXpuDJ1bVDadjKG");

/// Seasonal $SLEEP staking.
///
/// Rewards are a fixed pool per season, split pro-rata by weighted stake-seconds
/// and capped per wallet at an APR on raw stake-seconds.
#[program]
pub mod sleepagotchi_stake {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, stake_signer: Pubkey) -> Result<()> {
        instructions::initialize::handle_initialize(ctx, stake_signer)
    }

    pub fn open_season(ctx: Context<OpenSeason>, params: SeasonParams) -> Result<()> {
        instructions::open_season::handle_open_season(ctx, params)
    }

    pub fn update_season(ctx: Context<UpdateSeason>, params: SeasonParams) -> Result<()> {
        instructions::update_season::handle_update_season(ctx, params)
    }

    pub fn stake(ctx: Context<Stake>, amount: u64, multiplier_bps: u32) -> Result<()> {
        instructions::stake::handle_stake(ctx, amount, multiplier_bps)
    }

    pub fn unstake(ctx: Context<Unstake>, amount: u64) -> Result<()> {
        instructions::unstake::handle_unstake(ctx, amount)
    }

    pub fn claim(ctx: Context<Claim>) -> Result<()> {
        instructions::claim::handle_claim(ctx)
    }

    pub fn sweep_unclaimed(ctx: Context<SweepUnclaimed>) -> Result<()> {
        instructions::sweep::handle_sweep_unclaimed(ctx)
    }

    pub fn set_stake_signer(ctx: Context<AdminOnly>, stake_signer: Pubkey) -> Result<()> {
        instructions::admin::handle_set_stake_signer(ctx, stake_signer)
    }

    /// Proposes a handover, or cancels one with `None`. The admin does not
    /// change until `accept_admin`.
    pub fn transfer_admin(ctx: Context<AdminOnly>, new_admin: Option<Pubkey>) -> Result<()> {
        instructions::admin::handle_transfer_admin(ctx, new_admin)
    }

    /// Signed by the proposed admin, which is what proves the key exists.
    pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
        instructions::admin::handle_accept_admin(ctx)
    }

    pub fn set_paused(ctx: Context<AdminOnly>, paused: bool) -> Result<()> {
        instructions::admin::handle_set_paused(ctx, paused)
    }
}
