use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::ClaimError,
    events::{AdminTransferProposed, AdminTransferred, ClaimSignerUpdated, PausedSet},
    state::Config,
};

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    pub admin: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = admin)]
    pub config: Account<'info, Config>,
}

/// Signed by the proposed admin, not the current one — which is the entire point
/// of the two steps.
#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    pub pending_admin: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,
}

pub fn handle_set_claim_signer(ctx: Context<AdminOnly>, claim_signer: Pubkey) -> Result<()> {
    let previous = ctx.accounts.config.claim_signer;
    ctx.accounts.config.claim_signer = claim_signer;

    emit!(ClaimSignerUpdated {
        previous,
        current: claim_signer,
    });

    Ok(())
}

/// Proposes a handover, or cancels one with `None`. Changes nothing about who
/// currently holds the key.
pub fn handle_transfer_admin(ctx: Context<AdminOnly>, new_admin: Option<Pubkey>) -> Result<()> {
    ctx.accounts.config.pending_admin = new_admin;

    emit!(AdminTransferProposed {
        current: ctx.accounts.config.admin,
        pending: new_admin,
    });

    Ok(())
}

/// Completes a handover. Only the proposed key can call this, so an address that
/// was mistyped — or that nobody holds — never becomes the admin.
pub fn handle_accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
    let pending = ctx.accounts.pending_admin.key();
    require!(
        ctx.accounts.config.pending_admin == Some(pending),
        ClaimError::NotPendingAdmin
    );

    let previous = ctx.accounts.config.admin;
    ctx.accounts.config.admin = pending;
    ctx.accounts.config.pending_admin = None;

    emit!(AdminTransferred {
        previous,
        current: pending,
    });

    Ok(())
}

pub fn handle_set_paused(ctx: Context<AdminOnly>, paused: bool) -> Result<()> {
    ctx.accounts.config.paused = paused;

    emit!(PausedSet { paused });

    Ok(())
}
