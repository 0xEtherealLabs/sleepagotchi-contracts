use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::AirdropError,
    events::{AdminTransferProposed, AdminTransferred, PausedSet, RootUpdated, WindowUpdated},
    state::{validate_window, Config},
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

pub fn handle_set_root(ctx: Context<AdminOnly>, root: [u8; 32]) -> Result<()> {
    let previous = ctx.accounts.config.root;
    ctx.accounts.config.root = root;

    emit!(RootUpdated {
        previous,
        current: root,
    });

    Ok(())
}

pub fn handle_set_window(ctx: Context<AdminOnly>, start_ts: i64, end_ts: i64) -> Result<()> {
    validate_window(start_ts, end_ts)?;

    ctx.accounts.config.start_ts = start_ts;
    ctx.accounts.config.end_ts = end_ts;

    emit!(WindowUpdated { start_ts, end_ts });

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
        AirdropError::NotPendingAdmin
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
