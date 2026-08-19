//! LiteSVM tests for `stake` and `unstake`.

#![allow(clippy::result_large_err)]

mod common;

use anchor_lang::solana_program::instruction::Instruction;
use anchor_spl::token::spl_token;
use common::*;
use sleepagotchi_stake::{
    error::StakeError,
    events::{Staked, Unstaked},
    state::SeasonParams,
    ONE_X_BPS,
};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;

/// A world with `n` users and season 0 open and live.
fn live(n: usize) -> World {
    let mut world = World::with_users(n);
    world.open_season(params()).unwrap();
    world.warp_to(START);
    world
}

// -- staking -----------------------------------------------------------------

#[test]
fn staking_moves_tokens_into_the_vault_and_opens_a_position() {
    let mut world = live(1);
    let before = world.user_balance(0);

    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();

    let position = world.position(0, 0);
    assert_eq!(position.amount, MILLION);
    assert_eq!(position.multiplier_bps, ONE_X_BPS);
    assert_eq!(position.last_update_ts, START);
    assert!(!position.claimed);

    assert_eq!(world.season(0).total_staked, MILLION);
    assert_eq!(
        world.season(0).total_weighted,
        MILLION as u128 * ONE_X_BPS as u128
    );
    assert_eq!(world.balance(&world.stake_vault(0)), MILLION);
    assert_eq!(world.user_balance(0), before - MILLION);
}

#[test]
fn a_second_stake_adds_to_the_position_and_re_attests_the_multiplier() {
    let mut world = live(1);
    world.stake(0, 0, MILLION / 2, ONE_X_BPS).unwrap();
    world.warp_to(START + 10 * DAY);
    world.stake(0, 0, MILLION / 2, 3 * ONE_X_BPS).unwrap();

    let position = world.position(0, 0);
    assert_eq!(position.amount, MILLION);
    assert_eq!(position.multiplier_bps, 3 * ONE_X_BPS);
    // The first ten days stay priced at 1x — the new multiplier is not retroactive.
    assert_eq!(
        position.weighted_stake_seconds,
        (MILLION / 2) as u128 * ONE_X_BPS as u128 * (10 * DAY) as u128
    );
}

#[test]
fn accrual_through_the_instructions_matches_the_time_held() {
    let mut world = live(1);
    world.stake(0, 0, MILLION, 2 * ONE_X_BPS).unwrap();
    world.warp_to(START + 30 * DAY);
    world.unstake(0, 0, MILLION).unwrap();

    let position = world.position(0, 0);
    let expected = MILLION as u128 * (2 * ONE_X_BPS) as u128 * (30 * DAY) as u128;
    assert_eq!(position.weighted_stake_seconds, expected);
    assert_eq!(
        position.raw_stake_seconds,
        MILLION as u128 * (30 * DAY) as u128
    );
    assert_eq!(world.season(0).weighted_stake_seconds, expected);
}

#[test]
fn staking_refuses_zero() {
    let mut world = live(1);
    assert_error(world.stake(0, 0, 0, ONE_X_BPS), StakeError::ZeroAmount);
}

// -- the window --------------------------------------------------------------

#[test]
fn staking_is_refused_outside_the_window() {
    let mut world = World::with_users(1);
    world.open_season(params()).unwrap();

    world.warp_to(START - 1);
    assert_error(
        world.stake(0, 0, MILLION, ONE_X_BPS),
        StakeError::SeasonNotStarted,
    );

    world.warp_to(START);
    world.stake(0, 0, 1, ONE_X_BPS).unwrap();

    world.warp_to(END - 1);
    world.stake(0, 0, 1, ONE_X_BPS).unwrap();

    // The window is half-open: the closing instant is already outside it.
    world.warp_to(END);
    assert_error(world.stake(0, 0, 1, ONE_X_BPS), StakeError::SeasonEnded);

    world.warp_to(END + 365 * DAY);
    assert_error(world.stake(0, 0, 1, ONE_X_BPS), StakeError::SeasonEnded);
}

// -- authorization -----------------------------------------------------------

#[test]
fn staking_without_the_co_signature_fails() {
    let mut world = live(1);
    let user = world.users[0].insecure_clone();
    let signer = world.stake_signer.pubkey();
    let ix = world.stake_ix(&user.pubkey(), &signer, 0, MILLION, ONE_X_BPS);

    assert!(world
        .send_partially_signed(&[ix], &user, &[&user])
        .is_err());
}

#[test]
fn staking_with_the_wrong_signer_fails() {
    let mut world = live(1);
    let impostor = Keypair::new();

    assert_constraint(
        world.stake_signed_by(0, &impostor, impostor.pubkey(), 0, MILLION, ONE_X_BPS),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
}

/// Not redundant with the transfer's own authority check. An SPL delegate can
/// move tokens it does not own, so without `token::authority = user` an approved
/// delegate could stake someone else's balance into its own position and then
/// unstake it to itself. Requiring outright ownership closes that.
#[test]
fn a_delegate_cannot_stake_the_balance_it_was_approved_over() {
    let mut world = World::with_users(2);
    world.open_season(params()).unwrap();
    world.warp_to(START);

    let victim = world.users[1].insecure_clone();
    let thief = world.users[0].insecure_clone();
    let victim_tokens = world.token_account(&victim.pubkey());

    let approve = spl_token::instruction::approve(
        &spl_token::ID,
        &victim_tokens,
        &thief.pubkey(),
        &victim.pubkey(),
        &[],
        MILLION,
    )
    .unwrap();
    world.send(&[approve], &victim, &[&victim]).unwrap();

    let signer = world.stake_signer.insecure_clone();
    let mut ix = world.stake_ix(&thief.pubkey(), &signer.pubkey(), 0, MILLION, ONE_X_BPS);
    // `source` sits between the stake vault and the token program.
    let source = ix.accounts.len() - 3;
    ix.accounts[source].pubkey = victim_tokens;

    assert_constraint(
        world.send(&[ix], &thief, &[&thief, &signer]),
        anchor_lang::error::ErrorCode::ConstraintTokenOwner,
    );
    assert_eq!(world.user_balance(1), 10 * MILLION);
}

/// Rotating the signer invalidates anything the old key co-signed.
#[test]
fn a_rotated_out_signer_can_no_longer_authorize_a_stake() {
    let mut world = live(1);
    let old = world.stake_signer.insecure_clone();
    let new = Keypair::new();

    let admin = world.admin.insecure_clone();
    world
        .admin_ix(
            &admin,
            sleepagotchi_stake::instruction::SetStakeSigner {
                stake_signer: new.pubkey(),
            },
        )
        .unwrap();

    assert_constraint(
        world.stake_signed_by(0, &old, old.pubkey(), 0, MILLION, ONE_X_BPS),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
    world
        .stake_signed_by(0, &new, new.pubkey(), 0, MILLION, ONE_X_BPS)
        .unwrap();
}

// -- the multiplier ----------------------------------------------------------

#[test]
fn the_multiplier_is_bounded_at_both_ends() {
    let mut world = live(1);
    let max = params().max_multiplier_bps;

    assert_error(
        world.stake(0, 0, 1, ONE_X_BPS - 1),
        StakeError::MultiplierTooLow,
    );
    assert_error(world.stake(0, 0, 1, 0), StakeError::MultiplierTooLow);
    assert_error(
        world.stake(0, 0, 1, max + 1),
        StakeError::MultiplierTooHigh,
    );

    world.stake(0, 0, 1, max).unwrap();
    assert_eq!(world.position(0, 0).multiplier_bps, max);
}

// -- capacity ----------------------------------------------------------------

#[test]
fn the_per_wallet_maximum_binds() {
    let mut world = live(1);
    let cap = params().max_per_wallet;

    world.stake(0, 0, cap, ONE_X_BPS).unwrap();
    assert_error(
        world.stake(0, 0, 1, ONE_X_BPS),
        StakeError::WalletCapExceeded,
    );
    assert_eq!(world.position(0, 0).amount, cap);
}

#[test]
fn the_season_maximum_binds() {
    let mut world = World::with_users(3);
    world
        .open_season(SeasonParams {
            max_total_staked: 2 * MILLION,
            ..params()
        })
        .unwrap();
    world.warp_to(START);

    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.stake(1, 0, MILLION, ONE_X_BPS).unwrap();
    assert_error(world.stake(2, 0, 1, ONE_X_BPS), StakeError::SeasonFull);
    assert_eq!(world.season(0).total_staked, 2 * MILLION);
}

/// A running capacity check, not a latch: room freed by an unstake is usable
/// again. A latch would let one wallet close a season permanently by filling it
/// and leaving.
#[test]
fn capacity_freed_by_an_unstake_is_reusable() {
    let mut world = World::with_users(2);
    world
        .open_season(SeasonParams {
            max_total_staked: MILLION,
            ..params()
        })
        .unwrap();
    world.warp_to(START);

    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    assert_error(world.stake(1, 0, 1, ONE_X_BPS), StakeError::SeasonFull);

    world.warp_to(START + DAY);
    world.unstake(0, 0, MILLION).unwrap();
    world.stake(1, 0, MILLION, ONE_X_BPS).unwrap();

    assert_eq!(world.season(0).total_staked, MILLION);
}

// -- unstaking and custody ---------------------------------------------------

#[test]
fn unstaking_returns_the_tokens() {
    let mut world = live(1);
    let before = world.user_balance(0);

    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(START + DAY);
    world.unstake(0, 0, MILLION / 4).unwrap();

    assert_eq!(world.position(0, 0).amount, MILLION - MILLION / 4);
    assert_eq!(world.season(0).total_staked, MILLION - MILLION / 4);
    assert_eq!(world.balance(&world.stake_vault(0)), MILLION - MILLION / 4);
    assert_eq!(world.user_balance(0), before - MILLION + MILLION / 4);
}

#[test]
fn unstaking_refuses_more_than_is_held_and_refuses_zero() {
    let mut world = live(1);
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();

    assert_error(
        world.unstake(0, 0, MILLION + 1),
        StakeError::InsufficientStake,
    );
    assert_error(world.unstake(0, 0, 0), StakeError::ZeroAmount);
    assert_eq!(world.position(0, 0).amount, MILLION);
}

/// A guardrail rather than a security boundary — the caller is spending their
/// own principal either way — but it keeps a mistyped destination from being a
/// silent giveaway.
#[test]
fn unstaking_only_pays_an_account_the_caller_owns() {
    let mut world = World::with_users(2);
    world.open_season(params()).unwrap();
    world.warp_to(START);
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();

    let user = world.users[0].insecure_clone();
    let stranger = world.token_account(&world.users[1].pubkey());
    let mut ix = world.unstake_ix(&user.pubkey(), 0, MILLION);
    // `destination` is the second-to-last account.
    let destination = ix.accounts.len() - 2;
    ix.accounts[destination].pubkey = stranger;

    assert_constraint(
        world.send(&[ix], &user, &[&user]),
        anchor_lang::error::ErrorCode::ConstraintTokenOwner,
    );
}

/// The custody guarantee. Pausing is a stake-side switch and must never trap
/// principal.
#[test]
fn pausing_blocks_staking_but_never_unstaking() {
    let mut world = live(1);
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();

    world.set_paused(true);
    assert_error(world.stake(0, 0, 1, ONE_X_BPS), StakeError::Paused);

    world.warp_to(START + DAY);
    world.unstake(0, 0, MILLION).unwrap();
    assert_eq!(world.position(0, 0).amount, 0);
}

#[test]
fn unstaking_works_after_the_season_has_ended() {
    let mut world = live(1);
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();

    world.warp_to(END + 365 * DAY);
    world.unstake(0, 0, MILLION).unwrap();

    assert_eq!(world.position(0, 0).amount, 0);
    // Accrual stopped at the close, not at the withdrawal.
    assert_eq!(
        world.position(0, 0).raw_stake_seconds,
        MILLION as u128 * DURATION as u128
    );
}

#[test]
fn unstaking_keeps_the_stake_seconds_already_earned() {
    let mut world = live(1);
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(START + 10 * DAY);
    world.unstake(0, 0, MILLION).unwrap();

    let earned = world.position(0, 0).weighted_stake_seconds;
    assert_eq!(
        earned,
        MILLION as u128 * ONE_X_BPS as u128 * (10 * DAY) as u128
    );

    world.warp_to(END);
    assert_eq!(world.position(0, 0).weighted_stake_seconds, earned);
}

// -- seasons are independent -------------------------------------------------

#[test]
fn a_wallet_holds_a_separate_position_in_each_season() {
    let mut world = World::with_users(1);
    world.open_season(params()).unwrap();
    world.open_season(params()).unwrap();
    world.warp_to(START);

    world.stake(0, 0, MILLION / 2, ONE_X_BPS).unwrap();
    world.stake(0, 1, MILLION, 2 * ONE_X_BPS).unwrap();

    assert_ne!(
        world.position_address(0, &world.users[0].pubkey()),
        world.position_address(1, &world.users[0].pubkey())
    );
    assert_eq!(world.position(0, 0).amount, MILLION / 2);
    assert_eq!(world.position(1, 0).amount, MILLION);
    assert_eq!(world.position(1, 0).multiplier_bps, 2 * ONE_X_BPS);
    assert_eq!(world.balance(&world.stake_vault(0)), MILLION / 2);
    assert_eq!(world.balance(&world.stake_vault(1)), MILLION);
}

// -- rolling into the next season --------------------------------------------

/// Carrying a position into the next season needs no dedicated instruction:
/// `unstake` from the old and `stake` into the new, composed in one transaction,
/// is atomic, is a single wallet approval, and the tokens are never observably
/// in the user's wallet.
#[test]
fn a_position_rolls_into_the_next_season_in_one_transaction() {
    let mut world = World::with_users(1);
    world.open_season(params()).unwrap();
    // Back to back, so there is no gap in which rolling is impossible.
    world
        .open_season(SeasonParams {
            start_ts: END,
            end_ts: END + DURATION,
            ..params()
        })
        .unwrap();

    world.warp_to(START);
    world.stake(0, 0, MILLION, 2 * ONE_X_BPS).unwrap();
    let before = world.user_balance(0);

    world.warp_to(END);
    let user = world.users[0].insecure_clone();
    let signer = world.stake_signer.insecure_clone();
    let ixs = [
        world.unstake_ix(&user.pubkey(), 0, MILLION),
        world.stake_ix(&user.pubkey(), &signer.pubkey(), 1, MILLION, ONE_X_BPS),
    ];
    world.send(&ixs, &user, &[&user, &signer]).unwrap();

    // Principal moved vault to vault; the wallet balance never changed.
    assert_eq!(world.user_balance(0), before);
    assert_eq!(world.balance(&world.stake_vault(0)), 0);
    assert_eq!(world.balance(&world.stake_vault(1)), MILLION);

    // Season 0's accrual is left intact and is still claimable from season 0.
    assert_eq!(
        world.position(0, 0).weighted_stake_seconds,
        MILLION as u128 * (2 * ONE_X_BPS) as u128 * DURATION as u128
    );
    assert_eq!(world.position(0, 0).amount, 0);

    // Nothing carried across: the new position starts from zero, at a freshly
    // attested multiplier.
    assert_eq!(world.position(1, 0).amount, MILLION);
    assert_eq!(world.position(1, 0).multiplier_bps, ONE_X_BPS);
    assert_eq!(world.position(1, 0).weighted_stake_seconds, 0);
}

/// The two instructions share the user, config, mint and token program, and the
/// unstake destination is the stake source — so the composed transaction is far
/// inside the packet limit.
#[test]
fn the_rolling_transaction_fits_comfortably_in_one_packet() {
    let mut world = World::with_users(1);
    world.open_season(params()).unwrap();
    world
        .open_season(SeasonParams {
            start_ts: END,
            end_ts: END + DURATION,
            ..params()
        })
        .unwrap();

    let user = world.users[0].pubkey();
    let signer = world.stake_signer.pubkey();
    let ixs: [Instruction; 2] = [
        world.unstake_ix(&user, 0, MILLION),
        world.stake_ix(&user, &signer, 1, MILLION, ONE_X_BPS),
    ];

    let message = Message::new(&ixs, Some(&user));
    // Wire size: a shortvec signature count, one 64-byte signature each, then
    // the message itself.
    let size = 1 + 64 * message.header.num_required_signatures as usize + message.serialize().len();

    // Thirteen distinct accounts plus the program id.
    assert_eq!(message.account_keys.len(), 14);
    assert_eq!(message.header.num_required_signatures, 2);
    assert!(size < 1_232, "{size} bytes");
}

/// Rolling is a stake, so it is refused while staking is paused — and the
/// principal is still recoverable by unstaking alone.
#[test]
fn a_pause_blocks_the_roll_but_not_the_exit() {
    let mut world = World::with_users(1);
    world.open_season(params()).unwrap();
    world
        .open_season(SeasonParams {
            start_ts: END,
            end_ts: END + DURATION,
            ..params()
        })
        .unwrap();

    world.warp_to(START);
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.set_paused(true);
    world.warp_to(END);

    let user = world.users[0].insecure_clone();
    let signer = world.stake_signer.insecure_clone();
    let ixs = [
        world.unstake_ix(&user.pubkey(), 0, MILLION),
        world.stake_ix(&user.pubkey(), &signer.pubkey(), 1, MILLION, ONE_X_BPS),
    ];
    assert_error(world.send(&ixs, &user, &[&user, &signer]), StakeError::Paused);

    world.unstake(0, 0, MILLION).unwrap();
    assert_eq!(world.balance(&world.stake_vault(0)), 0);
}

// -- events ------------------------------------------------------------------

/// `amount` is this deposit; `position_amount` is the running total. A second
/// stake is where the two differ, and it also carries the re-attested
/// multiplier — the pair is what an indexer needs to rebuild a position.
#[test]
fn staking_emits_the_deposit_the_running_total_and_the_multiplier() {
    let mut world = live(1);

    let first = world.stake(0, 0, MILLION / 2, ONE_X_BPS);
    let event: Staked = only_event(&first);
    assert_eq!(event.user, world.users[0].pubkey());
    assert_eq!(event.season, 0);
    assert_eq!(event.amount, MILLION / 2);
    assert_eq!(event.position_amount, MILLION / 2);
    assert_eq!(event.multiplier_bps, ONE_X_BPS);
    assert_eq!(event.season_total_staked, MILLION / 2);

    world.warp_to(START + DAY);
    let second = world.stake(0, 0, MILLION / 2, 3 * ONE_X_BPS);
    let event: Staked = only_event(&second);
    assert_eq!(event.amount, MILLION / 2);
    assert_eq!(event.position_amount, MILLION);
    assert_eq!(event.multiplier_bps, 3 * ONE_X_BPS);
}

#[test]
fn unstaking_emits_the_withdrawal_and_what_is_left() {
    let mut world = live(1);
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(START + DAY);

    let result = world.unstake(0, 0, MILLION / 4);

    let event: Unstaked = only_event(&result);
    assert_eq!(event.user, world.users[0].pubkey());
    assert_eq!(event.amount, MILLION / 4);
    assert_eq!(event.position_amount, MILLION - MILLION / 4);
    assert_eq!(event.season_total_staked, MILLION - MILLION / 4);
}

/// Rolling a position forward is `unstake` + `stake` in one transaction, so the
/// log carries both halves and an indexer can tell it apart from an exit.
#[test]
fn a_roll_forward_emits_both_halves() {
    let mut world = live(1);
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END);
    world
        .open_season(SeasonParams {
            start_ts: END,
            end_ts: END + DURATION,
            ..params()
        })
        .unwrap();

    let user = world.users[0].insecure_clone();
    let signer = world.stake_signer.insecure_clone();
    let ixs = [
        world.unstake_ix(&user.pubkey(), 0, MILLION),
        world.stake_ix(&user.pubkey(), &signer.pubkey(), 1, MILLION, ONE_X_BPS),
    ];
    let result = world.send(&ixs, &user, &[&user, &signer]);

    let unstaked: Unstaked = only_event(&result);
    let staked: Staked = only_event(&result);
    assert_eq!((unstaked.season, unstaked.amount), (0, MILLION));
    assert_eq!((staked.season, staked.amount), (1, MILLION));
}
