use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    accrual::apply_stake,
    constants::*,
    error::StakeError,
    events::Staked,
    state::{Config, Position, Season},
};

#[derive(Accounts)]
pub struct Stake<'info> {
    /// Fee payer, and pays the position's rent on the first stake.
    #[account(mut)]
    pub user: Signer<'info>,
    /// Attests `multiplier_bps` against an NFT the program cannot see, on a
    /// chain it cannot reach. `Signer` plus the `has_one` below is the whole
    /// authorization check — the runtime has already verified the signature by
    /// the time this instruction runs.
    pub stake_signer: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = mint,
        has_one = stake_signer,
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [SEASON_SEED, season.id.to_le_bytes().as_ref()],
        bump = season.bump,
    )]
    pub season: Account<'info, Season>,
    #[account(
        init_if_needed,
        payer = user,
        space = 8 + Position::INIT_SPACE,
        seeds = [POSITION_SEED, season.id.to_le_bytes().as_ref(), user.key().as_ref()],
        bump
    )]
    pub position: Account<'info, Position>,
    pub mint: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = season,
    )]
    pub stake_vault: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mint, token::authority = user)]
    pub source: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// `multiplier_bps` is re-attested on every stake and applies from this instant
/// forward only — settlement inside `apply_stake` has already banked the elapsed
/// time at whatever was in force before.
pub fn handle_stake(ctx: Context<Stake>, amount: u64, multiplier_bps: u32) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    require!(!ctx.accounts.config.paused, StakeError::Paused);
    require!(amount > 0, StakeError::ZeroAmount);
    require!(
        now >= ctx.accounts.season.params.start_ts,
        StakeError::SeasonNotStarted
    );
    require!(
        now < ctx.accounts.season.params.end_ts,
        StakeError::SeasonEnded
    );
    require!(multiplier_bps >= ONE_X_BPS, StakeError::MultiplierTooLow);
    require!(
        multiplier_bps <= ctx.accounts.season.params.max_multiplier_bps,
        StakeError::MultiplierTooHigh
    );

    apply_stake(
        &mut ctx.accounts.season,
        &mut ctx.accounts.position,
        amount,
        multiplier_bps,
        now,
    )?;

    require!(
        ctx.accounts.position.amount <= ctx.accounts.season.params.max_per_wallet,
        StakeError::WalletCapExceeded
    );
    require!(
        ctx.accounts.season.total_staked <= ctx.accounts.season.params.max_total_staked,
        StakeError::SeasonFull
    );

    ctx.accounts.position.bump = ctx.bumps.position;

    transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.source.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.stake_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    emit!(Staked {
        user: ctx.accounts.user.key(),
        season: ctx.accounts.season.id,
        amount,
        multiplier_bps,
        position_amount: ctx.accounts.position.amount,
        season_total_staked: ctx.accounts.season.total_staked,
    });

    Ok(())
}
