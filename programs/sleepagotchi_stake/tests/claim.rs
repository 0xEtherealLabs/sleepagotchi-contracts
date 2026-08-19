//! LiteSVM tests for `claim` and `sweep_unclaimed`: the APR cap against real
//! vaults, and the solvency property that a season never pays out more than the
//! pool that funded it.

#![allow(clippy::result_large_err)]

mod common;

use common::*;
use sleepagotchi_stake::{
    error::StakeError,
    events::{RewardClaimed, SweptUnclaimed},
    state::SeasonParams,
    ONE_X_BPS,
};
use solana_keypair::Keypair;
use solana_signer::Signer;

fn live(n: usize, params: SeasonParams) -> World {
    let mut world = World::with_users(n);
    world.open_season(params).unwrap();
    world.warp_to(START);
    world
}

/// Small enough that the pro-rata split binds before the cap does, which is the
/// only regime in which the multiplier changes anyone's payout.
fn oversubscribed() -> SeasonParams {
    SeasonParams {
        reward_pool: 10_000 * UNIT,
        ..params()
    }
}

// -- the cap -----------------------------------------------------------------

/// A sole staker's pro-rata share is the whole pool. The cap is what they
/// actually receive, and the difference stays in the vault.
#[test]
fn a_sole_staker_receives_the_cap_and_the_surplus_stays_behind() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END);

    let before = world.user_balance(0);
    world.claim(0, 0).unwrap();

    let expected = apr_cap(
        MILLION as u128 * DURATION as u128,
        params().max_apr_bps,
    ) as u64;
    assert_eq!(expected, 36_986_301_369_863);
    assert_eq!(world.user_balance(0) - before, expected);
    assert_eq!(world.season(0).rewards_paid, expected);

    // The pool is barely touched, and none of it moved to anyone else.
    let pool = params().reward_pool;
    assert_eq!(world.balance(&world.reward_vault(0)), pool - expected);
    assert!(expected < pool / 50);
}

/// Both wallets hold the same tokens for the same time, so both cap at the same
/// number however they are weighted — even though the 4x wallet's raw share of
/// the pool is four times the other's.
#[test]
fn the_multiplier_does_not_lift_the_cap() {
    let mut world = live(2, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.stake(1, 0, MILLION, 4 * ONE_X_BPS).unwrap();
    world.warp_to(END);

    let plain_before = world.user_balance(0);
    let boosted_before = world.user_balance(1);
    world.claim(0, 0).unwrap();
    world.claim(1, 0).unwrap();

    let plain = world.user_balance(0) - plain_before;
    let boosted = world.user_balance(1) - boosted_before;

    assert_eq!(
        world.position(0, 1).weighted_stake_seconds,
        world.position(0, 0).weighted_stake_seconds * 4
    );
    assert_eq!(plain, boosted);
}

/// The other regime: with a pool small enough that the split binds first, the
/// multiplier is exactly what decides relative share.
#[test]
fn the_multiplier_decides_share_once_the_pool_binds() {
    let mut world = live(2, oversubscribed());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.stake(1, 0, MILLION, 4 * ONE_X_BPS).unwrap();
    world.warp_to(END);

    let plain_before = world.user_balance(0);
    let boosted_before = world.user_balance(1);
    world.claim(0, 0).unwrap();
    world.claim(1, 0).unwrap();

    let plain = world.user_balance(0) - plain_before;
    let boosted = world.user_balance(1) - boosted_before;

    assert_eq!(boosted, plain * 4);
    // One fifth and four fifths of a pool that is now fully spent.
    assert_eq!(plain + boosted, oversubscribed().reward_pool);
    assert_eq!(world.balance(&world.reward_vault(0)), 0);
}

/// The cap is measured on stake-seconds, so emptying a position before the close
/// does not shrink it. A cap on the closing balance would pay this wallet
/// nothing at all.
#[test]
fn a_wallet_that_unstaked_before_the_close_is_still_paid() {
    let mut world = live(2, oversubscribed());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.stake(1, 0, MILLION, ONE_X_BPS).unwrap();

    world.warp_to(END - 1);
    world.unstake(0, 0, MILLION).unwrap();

    world.warp_to(END);
    let before = world.user_balance(0);
    world.claim(0, 0).unwrap();

    assert_eq!(world.position(0, 0).amount, 0);
    assert!(world.user_balance(0) - before > 0);
}

#[test]
fn a_position_that_never_accrued_is_paid_nothing() {
    let mut world = live(2, oversubscribed());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    // Opened and emptied in the same slot: zero-length interval, zero weight.
    world.stake(1, 0, MILLION, 5 * ONE_X_BPS).unwrap();
    world.unstake(1, 0, MILLION).unwrap();

    world.warp_to(END);
    let before = world.user_balance(1);
    world.claim(1, 0).unwrap();

    assert_eq!(world.user_balance(1), before);
    assert!(world.position(0, 1).claimed);
}

// -- guards ------------------------------------------------------------------

#[test]
fn claiming_before_the_close_is_refused() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();

    world.warp_to(END - 1);
    assert_error(world.claim(0, 0), StakeError::SeasonNotEnded);

    world.warp_to(END);
    world.claim(0, 0).unwrap();
}

#[test]
fn a_position_can_only_be_claimed_once() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END);

    world.claim(0, 0).unwrap();
    let after = world.user_balance(0);

    assert_error(world.claim(0, 0), StakeError::AlreadyClaimed);
    assert_eq!(world.user_balance(0), after);
}

/// Rewards are earned, not granted. Pausing gates new stakes and reaches
/// nothing else.
#[test]
fn claiming_is_not_blocked_by_a_pause() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END);
    world.set_paused(true);

    let before = world.user_balance(0);
    world.claim(0, 0).unwrap();
    assert!(world.user_balance(0) > before);
}

#[test]
fn claiming_needs_no_co_signature() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END);

    // No `stake_signer` among the accounts at all, and the user signs alone.
    let user = world.users[0].insecure_clone();
    let ix = world.claim_ix(&user.pubkey(), 0);
    world.send(&[ix], &user, &[&user]).unwrap();
}

/// Claiming and unstaking are independent operations on the same position.
#[test]
fn principal_and_reward_are_collected_separately() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END);

    let before = world.user_balance(0);
    world.claim(0, 0).unwrap();
    let after_reward = world.user_balance(0);
    assert!(after_reward > before);
    // Claiming returned no principal.
    assert!(after_reward - before < MILLION);
    assert_eq!(world.balance(&world.stake_vault(0)), MILLION);

    world.unstake(0, 0, MILLION).unwrap();
    assert_eq!(world.user_balance(0), after_reward + MILLION);
    assert_eq!(world.balance(&world.stake_vault(0)), 0);
}

#[test]
fn claiming_only_pays_an_account_the_caller_owns() {
    let mut world = live(2, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END);

    let user = world.users[0].insecure_clone();
    let stranger = world.token_account(&world.users[1].pubkey());
    let mut ix = world.claim_ix(&user.pubkey(), 0);
    // `destination` sits between the reward vault and the token program.
    let destination = ix.accounts.len() - 2;
    ix.accounts[destination].pubkey = stranger;

    assert_constraint(
        world.send(&[ix], &user, &[&user]),
        anchor_lang::error::ErrorCode::ConstraintTokenOwner,
    );
}

/// The sweep takes the whole remainder, so a wallet that has not claimed by the
/// deadline finds nothing to claim. `InsufficientVault` is the only state in
/// which that check can fire — the vault is otherwise solvent by construction —
/// and it is what the delay exists to keep rare.
#[test]
fn a_claim_after_the_sweep_finds_an_empty_vault() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();

    world.warp_to(END + SWEEP_DELAY as i64);
    world.sweep(0).unwrap();
    assert_eq!(world.balance(&world.reward_vault(0)), 0);

    assert_error(world.claim(0, 0), StakeError::InsufficientVault);
    assert!(!world.position(0, 0).claimed);

    // Principal is untouched by any of it, and still recoverable.
    world.unstake(0, 0, MILLION).unwrap();
    assert_eq!(world.balance(&world.stake_vault(0)), 0);
}

// -- solvency ----------------------------------------------------------------

/// The property the whole design rests on, against real vaults: whatever the
/// traffic, a season pays out no more than the pool that funded it, and every
/// wallet is held to its own ceiling.
#[test]
fn a_season_never_pays_out_more_than_its_pool() {
    const USERS: usize = 5;
    let pool = 100_000 * UNIT;

    for seed in 0..4u64 {
        let mut rng = Rng::new(seed);
        let mut world = live(
            USERS,
            SeasonParams {
                reward_pool: pool,
                ..params()
            },
        );

        let mut now = START;
        for _ in 0..40 {
            now += rng.below((3 * DAY) as u64) as i64;
            if now >= END {
                break;
            }
            world.warp_to(now);

            let index = rng.below(USERS as u64) as usize;
            let held = world
                .svm
                .get_account(&world.position_address(0, &world.users[index].pubkey()))
                .map(|_| world.position(0, index).amount)
                .unwrap_or(0);

            if held > 0 && rng.below(3) == 0 {
                let amount = rng.next_u64() % (held + 1);
                if amount > 0 {
                    world.unstake(index, 0, amount).unwrap();
                }
            } else {
                let room = MILLION - held;
                let amount = rng.next_u64() % (room + 1);
                if amount > 0 {
                    let multiplier = ONE_X_BPS + rng.below(40_001) as u32;
                    world.stake(index, 0, amount, multiplier).unwrap();
                }
            }
        }

        world.warp_to(END);

        let mut paid = 0u64;
        for index in 0..USERS {
            if world
                .svm
                .get_account(&world.position_address(0, &world.users[index].pubkey()))
                .is_none()
            {
                continue;
            }
            let before = world.user_balance(index);
            world.claim(index, 0).unwrap();
            let received = world.user_balance(index) - before;
            paid += received;

            let position = world.position(0, index);
            let cap = apr_cap(position.raw_stake_seconds, params().max_apr_bps);
            assert!(received as u128 <= cap, "seed {seed}: over the cap");
        }

        assert!(paid <= pool, "seed {seed}: paid {paid} of {pool}");
        assert_eq!(world.season(0).rewards_paid, paid, "seed {seed}");
        assert_eq!(
            world.balance(&world.reward_vault(0)),
            pool - paid,
            "seed {seed}"
        );
        // Principal is untouched by any of it.
        assert_eq!(
            world.balance(&world.stake_vault(0)),
            world.season(0).total_staked
        );
    }
}

// -- sweep -------------------------------------------------------------------

/// No waiting period: the only gate is that the season has closed, and that one
/// stops a sweep taking the pool out from under people who have accrued against
/// it and cannot claim yet.
#[test]
fn sweeping_is_refused_until_the_claim_window_runs_and_then_takes_the_remainder() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();

    world.warp_to(END - 1);
    assert_error(world.sweep(0), StakeError::SweepTooEarly);

    // The close is no longer enough on its own.
    world.warp_to(END);
    assert_error(world.sweep(0), StakeError::SweepTooEarly);

    world.claim(0, 0).unwrap();

    world.warp_to(END + SWEEP_DELAY as i64 - 1);
    assert_error(world.sweep(0), StakeError::SweepTooEarly);
    world.warp_to(END + SWEEP_DELAY as i64);

    let remainder = world.balance(&world.reward_vault(0));
    let treasury = world.balance(&world.treasury);
    world.sweep(0).unwrap();

    assert_eq!(world.balance(&world.reward_vault(0)), 0);
    assert_eq!(world.balance(&world.treasury), treasury + remainder);
    assert!(world.season(0).swept);
}

/// The finding the delay exists for. Claims are impossible before `end_ts`, so
/// at the close every wallet is unclaimed by construction — without a delay one
/// admin transaction at `end_ts + 1` would take every reward that had been
/// earned, leaving the entitlement on chain and unpayable.
#[test]
fn the_close_alone_does_not_let_a_sweep_cancel_everyone_s_reward() {
    let mut world = live(2, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.stake(1, 0, MILLION, ONE_X_BPS).unwrap();

    world.warp_to(END);
    assert_error(world.sweep(0), StakeError::SweepTooEarly);

    // The whole claim window is protected, not just its first second.
    for offset in [1, SWEEP_DELAY as i64 / 2, SWEEP_DELAY as i64 - 1] {
        world.warp_to(END + offset);
        assert_error(world.sweep(0), StakeError::SweepTooEarly);
    }

    // So a wallet that has not touched the program since staking is still paid.
    world.claim(0, 0).unwrap();
    world.claim(1, 0).unwrap();
    assert!(world.user_balance(0) > 0);
    assert!(world.user_balance(1) > 0);
}

/// Past the window the admin's timing is unrestricted again, and a wallet that
/// left it too late still loses its reward. The delay bounds that exposure; it
/// does not remove it.
#[test]
fn sweeping_after_the_window_still_strands_a_late_claimant() {
    let mut world = live(2, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.stake(1, 0, MILLION, ONE_X_BPS).unwrap();

    world.warp_to(END);
    world.claim(0, 0).unwrap();

    world.warp_to(END + SWEEP_DELAY as i64);
    world.sweep(0).unwrap();

    assert_error(world.claim(1, 0), StakeError::InsufficientVault);
}

#[test]
fn a_season_can_only_be_swept_once() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END + SWEEP_DELAY as i64);

    world.sweep(0).unwrap();
    assert_error(world.sweep(0), StakeError::AlreadySwept);
}

#[test]
fn only_the_admin_can_sweep() {
    let mut world = live(1, params());
    world.warp_to(END);

    let impostor = Keypair::new();
    world.svm.airdrop(&impostor.pubkey(), 10 * SOL).unwrap();
    // A real token account, so the refusal is the admin check and not a missing
    // destination.
    world.mint_to(&impostor.pubkey(), UNIT);
    let destination = world.token_account(&impostor.pubkey());

    assert_constraint(
        world.sweep_as(&impostor, 0, destination),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
}

/// The sweep reaches the reward vault through its own authority, so principal
/// is not an account it can be pointed at.
#[test]
fn a_sweep_cannot_be_aimed_at_the_stake_vault() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END);

    let admin = world.admin.insecure_clone();
    let mut ix = world.sweep_ix(&admin.pubkey(), 0, world.treasury);
    // `reward_vault` sits between the mint and the destination.
    let vault = ix.accounts.len() - 3;
    ix.accounts[vault].pubkey = world.stake_vault(0);

    assert_constraint(
        world.send(&[ix], &admin, &[&admin]),
        anchor_lang::error::ErrorCode::ConstraintTokenOwner,
    );
    assert_eq!(world.balance(&world.stake_vault(0)), MILLION);
}

/// Rolling a remainder into a later season is a sweep plus an `open_season`,
/// which is why the destination is unconstrained beyond its mint.
#[test]
fn a_remainder_can_be_swept_into_a_later_seasons_vault() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();

    // The next season starts when the old one's claim window closes, which is the
    // earliest its remainder can be rolled forward into.
    world.warp_to(END);
    world
        .open_season(SeasonParams {
            start_ts: END + SWEEP_DELAY as i64,
            end_ts: END + SWEEP_DELAY as i64 + DURATION,
            ..params()
        })
        .unwrap();
    world.warp_to(END + SWEEP_DELAY as i64);

    let remainder = world.balance(&world.reward_vault(0));
    let next = world.balance(&world.reward_vault(1));

    let admin = world.admin.insecure_clone();
    let destination = world.reward_vault(1);
    world.sweep_as(&admin, 0, destination).unwrap();

    assert_eq!(world.balance(&world.reward_vault(0)), 0);
    assert_eq!(world.balance(&world.reward_vault(1)), next + remainder);
}

// -- events ------------------------------------------------------------------

/// The payout is re-derivable from the log alone: both of the position's
/// integrals and the season's denominator travel with the event, which is the
/// whole point for a program whose state is a time integral.
#[test]
fn a_claim_emits_the_payout_and_the_integrals_behind_it() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END);

    let result = world.claim(0, 0);

    let event: RewardClaimed = only_event(&result);
    let position = world.position(0, 0);
    let season = world.season(0);

    assert_eq!(event.user, world.users[0].pubkey());
    assert_eq!(event.season, 0);
    assert_eq!(event.owed, season.rewards_paid);
    assert_eq!(event.weighted_stake_seconds, position.weighted_stake_seconds);
    assert_eq!(event.raw_stake_seconds, position.raw_stake_seconds);
    assert_eq!(
        event.season_weighted_stake_seconds,
        season.weighted_stake_seconds
    );
}

/// A position that earned nothing is still claimed, and the log says so rather
/// than going quiet on the early return.
#[test]
fn a_zero_payout_still_emits() {
    let mut world = live(2, oversubscribed());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.stake(1, 0, MILLION, ONE_X_BPS).unwrap();
    world.unstake(1, 0, MILLION).unwrap();
    world.warp_to(END);

    let result = world.claim(1, 0);

    let event: RewardClaimed = only_event(&result);
    assert_eq!(event.owed, 0);
    assert_eq!(event.weighted_stake_seconds, 0);
}

/// The one an operator should always be alerted on.
#[test]
fn a_sweep_emits_the_amount_and_where_it_went() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END + SWEEP_DELAY as i64);

    let remainder = world.balance(&world.reward_vault(0));
    let result = world.sweep(0);

    let event: SweptUnclaimed = only_event(&result);
    assert_eq!(event.season, 0);
    assert_eq!(event.amount, remainder);
    assert_eq!(event.destination, world.treasury);
}

/// A sweep that takes nothing still latches `swept`, so it is still an event.
#[test]
fn a_sweep_of_an_empty_vault_still_emits() {
    let mut world = live(2, oversubscribed());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.stake(1, 0, MILLION, 4 * ONE_X_BPS).unwrap();
    world.warp_to(END);
    world.claim(0, 0).unwrap();
    world.claim(1, 0).unwrap();
    assert_eq!(world.balance(&world.reward_vault(0)), 0);

    world.warp_to(END + SWEEP_DELAY as i64);
    let result = world.sweep(0);

    let event: SweptUnclaimed = only_event(&result);
    assert_eq!(event.amount, 0);
    assert!(world.season(0).swept);
}

/// A refused transaction emits nothing — the log records what happened, not what
/// was attempted.
#[test]
fn a_refused_sweep_emits_nothing() {
    let mut world = live(1, params());
    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    world.warp_to(END);

    let result = world.sweep(0);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .meta
        .logs
        .iter()
        .all(|line| !line.starts_with("Program data: ")));
}
