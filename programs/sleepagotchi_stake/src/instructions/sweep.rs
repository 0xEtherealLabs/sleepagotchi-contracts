use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    constants::*,
    error::StakeError,
    events::SweptUnclaimed,
    state::{Config, Season},
};

/// Moves a closed season's unclaimed remainder out.
///
/// Reachable only through the reward vault's authority, so the stake vault is
/// not an account this instruction can name — a sweep cannot touch principal
/// whatever it is pointed at.
#[derive(Accounts)]
pub struct SweepUnclaimed<'info> {
    pub admin: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin, has_one = mint)]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [SEASON_SEED, season.id.to_le_bytes().as_ref()],
        bump = season.bump,
    )]
    pub season: Account<'info, Season>,
    /// CHECK: signs the transfer out. Checked by seeds against the stored bump.
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
    /// Treasury, or the reward vault of a later season — rolling a remainder
    /// forward is a sweep plus an `open_season`, not a separate instruction.
    #[account(mut, token::mint = mint)]
    pub destination: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

/// Takes whatever is left, once the claim window has run.
///
/// The sweep takes the whole remaining vault, including rewards owed to wallets
/// that have not claimed. `sweep_delay_seconds` is what stops that being
/// available the instant a season closes: claims are impossible before `end_ts`,
/// so without the delay there is always an interval in which the entire pool is
/// sweepable and nobody has collected a thing.
///
/// The value is frozen with every other season parameter, so the window a
/// staker is promised when they stake is the window they get. Past it the timing
/// is unrestricted, and a sweep still costs a late claimant its reward: the delay
/// bounds that exposure rather than removing it.
pub fn handle_sweep_unclaimed(ctx: Context<SweepUnclaimed>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    require!(
        now >= ctx.accounts.season.params.sweep_after()?,
        StakeError::SweepTooEarly
    );
    require!(!ctx.accounts.season.swept, StakeError::AlreadySwept);

    ctx.accounts.season.swept = true;

    let remainder = ctx.accounts.reward_vault.amount;

    // Before the early return: `swept` latched either way, and an operator
    // alerting on this should see the sweep that took nothing too.
    emit!(SweptUnclaimed {
        season: ctx.accounts.season.id,
        amount: remainder,
        destination: ctx.accounts.destination.key(),
    });

    if remainder == 0 {
        return Ok(());
    }

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
        remainder,
        ctx.accounts.mint.decimals,
    )
}
