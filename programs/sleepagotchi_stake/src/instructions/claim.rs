use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    accrual::{reward, settle},
    constants::*,
    error::StakeError,
    events::RewardClaimed,
    state::{Config, Position, Season},
};

/// No co-signature and no pause check. What a position earned is decided
/// entirely by state the program itself maintained, so there is nothing left for
/// a backend to attest by the time this runs.
#[derive(Accounts)]
pub struct Claim<'info> {
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
    /// CHECK: signs the payout. Checked by seeds against the season's stored bump.
    #[account(
        seeds = [REWARDS_SEED, season.id.to_le_bytes().as_ref()],
        bump = season.rewards_bump
    )]
    pub rewards_authority: UncheckedAccount<'info>,
    pub mint: Account<'info, Mint>,
    /// Rewards only. The stake vault has a different authority and is not an
    /// account this instruction can name.
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = rewards_authority,
    )]
    pub reward_vault: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mint, token::authority = user)]
    pub destination: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

/// Pays a position's whole reward, once, after the season closes.
///
/// Principal is not returned here. Claiming and unstaking are independent, so a
/// wallet that withdrew in week one still collects its share in week twelve.
pub fn handle_claim(ctx: Context<Claim>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    require!(
        now >= ctx.accounts.season.params.end_ts,
        StakeError::SeasonNotEnded
    );
    require!(!ctx.accounts.position.claimed, StakeError::AlreadyClaimed);

    // Clamped to `end_ts`, so this is the final value for both accumulators
    // however long ago either was last touched.
    settle(
        &mut ctx.accounts.season,
        &mut ctx.accounts.position,
        now,
    )?;
    require!(
        ctx.accounts.season.weighted_stake_seconds > 0,
        StakeError::NothingStaked
    );

    let owed = reward(&ctx.accounts.season, &ctx.accounts.position)?;

    ctx.accounts.position.claimed = true;
    ctx.accounts.season.rewards_paid = ctx
        .accounts
        .season
        .rewards_paid
        .checked_add(owed)
        .ok_or(StakeError::MathOverflow)?;

    // Before the early return: a position that earned nothing is still claimed,
    // and the log should say so rather than going quiet.
    emit!(RewardClaimed {
        user: ctx.accounts.user.key(),
        season: ctx.accounts.season.id,
        owed,
        weighted_stake_seconds: ctx.accounts.position.weighted_stake_seconds,
        raw_stake_seconds: ctx.accounts.position.raw_stake_seconds,
        season_weighted_stake_seconds: ctx.accounts.season.weighted_stake_seconds,
    });

    if owed == 0 {
        return Ok(());
    }
    require!(
        ctx.accounts.reward_vault.amount >= owed,
        StakeError::InsufficientVault
    );

    let id = ctx.accounts.season.id.to_le_bytes();
    let bump = &[ctx.accounts.season.rewards_bump];
    let signer_seeds: &[&[&[u8]]] = &[&[REWARDS_SEED, id.as_ref(), bump]];

    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.reward_vault.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.destination.to_account_info(),
                authority: ctx.accounts.rewards_authority.to_account_info(),
            },
            signer_seeds,
        ),
        owed,
        ctx.accounts.mint.decimals,
    )
}
