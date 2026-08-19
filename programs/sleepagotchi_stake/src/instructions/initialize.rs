use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

use crate::{constants::*, state::Config};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, Config>,
    /// Fixed here for the life of the program. Every season's vaults are token
    /// accounts for this mint, so a config that could switch it would strand
    /// balances behind seasons that no longer match.
    pub mint: Account<'info, Mint>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize(ctx: Context<Initialize>, stake_signer: Pubkey) -> Result<()> {
    ctx.accounts.config.set_inner(Config {
        admin: ctx.accounts.admin.key(),
        pending_admin: None,
        stake_signer,
        mint: ctx.accounts.mint.key(),
        paused: false,
        next_season: 0,
        bump: ctx.bumps.config,
    });

    Ok(())
}
