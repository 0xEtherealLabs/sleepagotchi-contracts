use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    constants::*,
    error::StakeError,
    events::SeasonUpdated,
    state::{Config, Season, SeasonParams},
};

#[derive(Accounts)]
pub struct UpdateSeason<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = admin,
        has_one = mint,
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [SEASON_SEED, season.id.to_le_bytes().as_ref()],
        bump = season.bump,
    )]
    pub season: Account<'info, Season>,
    /// CHECK: signs the refund when the pool is lowered. Checked by seeds.
    #[account(
        seeds = [REWARDS_SEED, season.id.to_le_bytes().as_ref()],
        bump = season.rewards_bump
    )]
    pub rewards_authority: UncheckedAccount<'info>,
    pub mint: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = rewards_authority,
    )]
    pub reward_vault: Account<'info, TokenAccount>,
    /// Source when the pool goes up, destination when it goes down.
    #[account(mut, token::mint = mint, token::authority = admin)]
    pub funding: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

/// Rewrite a season's parameters, in full, before it starts.
///
/// The single check below is the entire safety argument. `stake` requires
/// `now >= start_ts`, so before a season starts no position exists and both
/// accumulators are zero — there is no accrual for a parameter change to
/// invalidate, and therefore nothing to reason about field by field.
///
/// After `start_ts` a season is immutable in full. Every parameter would
/// retroactively reprice time that stakers have already committed, which is the
/// one thing a staker needs a guarantee against.
pub fn handle_update_season(ctx: Context<UpdateSeason>, params: SeasonParams) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        now < ctx.accounts.season.params.start_ts,
        StakeError::SeasonStarted
    );
    params.validate(now)?;

    let previous = ctx.accounts.season.params.reward_pool;
    ctx.accounts.season.params = params;

    emit!(SeasonUpdated {
        id: ctx.accounts.season.id,
        params,
    });
    ctx.accounts.season.last_update_ts = params.start_ts;

    // The vault tracks the pool rather than being reconciled later, so
    // `reward_vault.amount == params.reward_pool` survives an edit.
    let decimals = ctx.accounts.mint.decimals;
    match params.reward_pool.cmp(&previous) {
        std::cmp::Ordering::Greater => transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from: ctx.accounts.funding.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.reward_vault.to_account_info(),
                    authority: ctx.accounts.admin.to_account_info(),
                },
            ),
            params.reward_pool - previous,
            decimals,
        ),
        std::cmp::Ordering::Less => {
            let id = ctx.accounts.season.id.to_le_bytes();
            let bump = &[ctx.accounts.season.rewards_bump];
            let signer_seeds: &[&[&[u8]]] = &[&[REWARDS_SEED, id.as_ref(), bump]];

            transfer_checked(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.key(),
                    TransferChecked {
                        from: ctx.accounts.reward_vault.to_account_info(),
                        mint: ctx.accounts.mint.to_account_info(),
                        to: ctx.accounts.funding.to_account_info(),
                        authority: ctx.accounts.rewards_authority.to_account_info(),
                    },
                    signer_seeds,
                ),
                previous - params.reward_pool,
                decimals,
            )
        }
        std::cmp::Ordering::Equal => Ok(()),
    }
}
