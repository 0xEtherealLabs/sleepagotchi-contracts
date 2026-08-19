use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    constants::*,
    error::AirdropError,
    events::Claimed,
    merkle,
    state::{Config, Receipt},
};

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = mint)]
    pub config: Account<'info, Config>,
    /// `init` is the replay check: it fails if this already exists.
    #[account(
        init,
        payer = user,
        space = 8 + Receipt::INIT_SPACE,
        seeds = [RECEIPT_SEED, user.key().as_ref()],
        bump
    )]
    pub receipt: Account<'info, Receipt>,
    pub mint: Account<'info, Mint>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = config,
    )]
    pub vault: Account<'info, TokenAccount>,
    /// Created by a `create_idempotent` instruction ahead of this one, so the user
    /// pays the rent.
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = user,
    )]
    pub destination: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_claim(ctx: Context<Claim>, amount: u64, proof: Vec<[u8; 32]>) -> Result<()> {
    let config = &ctx.accounts.config;
    let now = Clock::get()?.unix_timestamp;

    require!(!config.paused, AirdropError::Paused);
    require!(now >= config.start_ts, AirdropError::NotStarted);
    require!(now < config.end_ts, AirdropError::Ended);
    require!(
        merkle::verify(&config.root, &ctx.accounts.user.key(), amount, &proof),
        AirdropError::InvalidProof
    );
    require!(
        ctx.accounts.vault.amount >= amount,
        AirdropError::InsufficientVault
    );

    let bump = &[config.bump];
    let signer_seeds: &[&[&[u8]]] = &[&[CONFIG_SEED, bump]];

    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.vault.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.destination.to_account_info(),
                authority: config.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    emit!(Claimed {
        user: ctx.accounts.user.key(),
        amount,
    });

    Ok(())
}
