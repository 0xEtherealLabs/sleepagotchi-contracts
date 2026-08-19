use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked},
};

use crate::{
    constants::*,
    events::SeasonOpened,
    state::{Config, Season, SeasonParams},
};

#[derive(Accounts)]
pub struct OpenSeason<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin,
        has_one = mint,
    )]
    pub config: Account<'info, Config>,
    /// Id comes from the config's allocator rather than from input, so seasons
    /// are sequential and no id can be skipped or reused.
    #[account(
        init,
        payer = admin,
        space = 8 + Season::INIT_SPACE,
        seeds = [SEASON_SEED, config.next_season.to_le_bytes().as_ref()],
        bump
    )]
    pub season: Account<'info, Season>,
    /// CHECK: authority over the reward vault and nothing else. Its only job is
    /// to be a different owner from `season`, which is what keeps principal and
    /// rewards in separate token accounts.
    #[account(
        seeds = [REWARDS_SEED, config.next_season.to_le_bytes().as_ref()],
        bump
    )]
    pub rewards_authority: UncheckedAccount<'info>,
    pub mint: Account<'info, Mint>,
    /// Principal, and only principal. No instruction can move it except
    /// `unstake`.
    #[account(
        init,
        payer = admin,
        associated_token::mint = mint,
        associated_token::authority = season,
    )]
    pub stake_vault: Account<'info, TokenAccount>,
    /// Holds exactly `params.reward_pool` from here on.
    #[account(
        init,
        payer = admin,
        associated_token::mint = mint,
        associated_token::authority = rewards_authority,
    )]
    pub reward_vault: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mint, token::authority = admin)]
    pub funding: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

/// Creates a season and funds its reward pool in one instruction.
///
/// Funding is not a separate step on purpose: `reward_vault.amount ==
/// params.reward_pool` then holds from the moment the season exists, so there is
/// no window in which a season is open and under-funded, and nothing later has
/// to check that it was topped up.
pub fn handle_open_season(ctx: Context<OpenSeason>, params: SeasonParams) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    params.validate(now)?;

    let id = ctx.accounts.config.next_season;

    ctx.accounts.season.set_inner(Season {
        id,
        params,
        total_staked: 0,
        total_weighted: 0,
        weighted_stake_seconds: 0,
        // Accrual is measured from the start, not from now: a season opened
        // early has no stakers to credit for the gap.
        last_update_ts: params.start_ts,
        rewards_paid: 0,
        swept: false,
        bump: ctx.bumps.season,
        rewards_bump: ctx.bumps.rewards_authority,
    });

    ctx.accounts.config.next_season = id
        .checked_add(1)
        .ok_or(crate::error::StakeError::MathOverflow)?;

    transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.funding.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.reward_vault.to_account_info(),
                authority: ctx.accounts.admin.to_account_info(),
            },
        ),
        params.reward_pool,
        ctx.accounts.mint.decimals,
    )?;

    emit!(SeasonOpened { id, params });

    Ok(())
}
