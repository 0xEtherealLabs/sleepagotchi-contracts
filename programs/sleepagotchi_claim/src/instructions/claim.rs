use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    constants::*,
    error::ClaimError,
    events::Claimed,
    state::{Config, Receipt},
};

#[derive(Accounts)]
pub struct Claim<'info> {
    /// Fee payer, and pays the receipt rent on the first claim.
    #[account(mut)]
    pub user: Signer<'info>,
    /// `Signer` plus `has_one` below is the whole authorization check.
    pub claim_signer: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = mint,
        has_one = claim_signer,
    )]
    pub config: Account<'info, Config>,
    #[account(
        init_if_needed,
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
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = user,
    )]
    pub destination: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// `total` is the signed running total, not a delta, so resubmitting a transaction
/// fails `NothingToClaim` rather than paying twice.
pub fn handle_claim(ctx: Context<Claim>, total: u64) -> Result<()> {
    let config = &ctx.accounts.config;
    let claimed = ctx.accounts.receipt.claimed;

    require!(!config.paused, ClaimError::Paused);
    require!(total > claimed, ClaimError::NothingToClaim);

    let owed = total - claimed;
    require!(
        ctx.accounts.vault.amount >= owed,
        ClaimError::InsufficientVault
    );

    ctx.accounts.receipt.claimed = total;

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
        owed,
        ctx.accounts.mint.decimals,
    )?;

    emit!(Claimed {
        user: ctx.accounts.user.key(),
        owed,
        total,
    });

    Ok(())
}
