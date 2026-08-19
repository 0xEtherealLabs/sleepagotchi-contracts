use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    accrual::apply_unstake,
    constants::*,
    error::StakeError,
    events::Unstaked,
    state::{Config, Position, Season},
};

/// No `stake_signer`, no `paused` check, no window check.
///
/// This is the custody guarantee, and it is deliberate in every one of those
/// three omissions: principal must come back if the backend is offline, if the
/// admin key is lost, if the program is halted, or if the season turned out to
/// be misconfigured. A season being frozen never means a wallet's money is.
#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = mint)]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [SEASON_SEED, season.id.to_le_bytes().as_ref()],
        bump = season.bump,
    )]
    pub season: Account<'info, Season>,
    #[account(
        mut,
        seeds = [POSITION_SEED, season.id.to_le_bytes().as_ref(), user.key().as_ref()],
        bump = position.bump,
    )]
    pub position: Account<'info, Position>,
    pub mint: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = season,
    )]
    pub stake_vault: Account<'info, TokenAccount>,
    /// Any token account the caller owns, so unstaking never depends on an ATA
    /// existing.
    #[account(mut, token::mint = mint, token::authority = user)]
    pub destination: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

/// Accrued stake-seconds are untouched: time already served is already earned,
/// and the reward for it is claimed from this season after it closes whether or
/// not the position is still funded.
pub fn handle_unstake(ctx: Context<Unstake>, amount: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    require!(amount > 0, StakeError::ZeroAmount);

    apply_unstake(
        &mut ctx.accounts.season,
        &mut ctx.accounts.position,
        amount,
        now,
    )?;

    let id = ctx.accounts.season.id.to_le_bytes();
    let bump = &[ctx.accounts.season.bump];
    let signer_seeds: &[&[&[u8]]] = &[&[SEASON_SEED, id.as_ref(), bump]];

    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.stake_vault.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.destination.to_account_info(),
                authority: ctx.accounts.season.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    emit!(Unstaked {
        user: ctx.accounts.user.key(),
        season: ctx.accounts.season.id,
        amount,
        position_amount: ctx.accounts.position.amount,
        season_total_staked: ctx.accounts.season.total_staked,
    });

    Ok(())
}
