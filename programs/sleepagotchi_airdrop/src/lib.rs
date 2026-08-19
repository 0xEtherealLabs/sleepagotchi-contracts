pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod merkle;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use events::*;
pub use instructions::*;
pub use state::*;

declare_id!("DM6GCMdgn9daiiUCCaiCjrQHWwe3RiNJK3EPcqa2sdBg");

#[program]
pub mod sleepagotchi_airdrop {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        root: [u8; 32],
        start_ts: i64,
        end_ts: i64,
    ) -> Result<()> {
        instructions::initialize::handle_initialize(ctx, root, start_ts, end_ts)
    }

    pub fn claim(ctx: Context<Claim>, amount: u64, proof: Vec<[u8; 32]>) -> Result<()> {
        instructions::claim::handle_claim(ctx, amount, proof)
    }

    pub fn set_root(ctx: Context<AdminOnly>, root: [u8; 32]) -> Result<()> {
        instructions::admin::handle_set_root(ctx, root)
    }

    pub fn set_window(ctx: Context<AdminOnly>, start_ts: i64, end_ts: i64) -> Result<()> {
        instructions::admin::handle_set_window(ctx, start_ts, end_ts)
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

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        instructions::withdraw::handle_withdraw(ctx, amount)
    }
}
