pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use events::*;
pub use instructions::*;
pub use state::*;

declare_id!("8igHauFjDNKpTkyATw9cigy7XWxWMbvHotA4S9i4qe1f");

#[program]
pub mod sleepagotchi_claim {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, claim_signer: Pubkey) -> Result<()> {
        instructions::initialize::handle_initialize(ctx, claim_signer)
    }

    pub fn claim(ctx: Context<Claim>, total: u64) -> Result<()> {
        instructions::claim::handle_claim(ctx, total)
    }

    pub fn set_claim_signer(ctx: Context<AdminOnly>, claim_signer: Pubkey) -> Result<()> {
        instructions::admin::handle_set_claim_signer(ctx, claim_signer)
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
