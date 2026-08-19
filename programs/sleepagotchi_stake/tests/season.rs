//! LiteSVM tests for the season lifecycle: `initialize`, `open_season`,
//! `update_season` and the admin setters.

#![allow(clippy::result_large_err)]

mod common;

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, program_pack::Pack},
    InstructionData, ToAccountMetas,
};
use anchor_spl::token::spl_token;
use common::*;
use sleepagotchi_stake::{
    accounts,
    error::StakeError,
    events::{
        AdminTransferProposed, AdminTransferred, PausedSet, SeasonOpened, SeasonUpdated,
        StakeSignerUpdated,
    },
    instruction,
    state::SeasonParams,
    ONE_X_BPS,
};
use solana_keypair::Keypair;
use solana_signer::Signer;

// -- initialize --------------------------------------------------------------

#[test]
fn initialize_sets_the_config_and_starts_unpaused() {
    let world = World::new();
    let config = world.config();

    assert_eq!(config.admin, world.admin.pubkey());
    assert_eq!(config.stake_signer, world.stake_signer.pubkey());
    assert_eq!(config.mint, world.mint);
    assert!(!config.paused);
    assert_eq!(config.next_season, 0);
}

#[test]
fn initialize_is_a_singleton() {
    let mut world = World::new();
    // Second call fails inside `init`, so the error is the system program's.
    assert_code(world.initialize(), 0);
}

// -- open_season -------------------------------------------------------------

#[test]
fn open_season_creates_a_funded_season() {
    let mut world = World::new();
    let before = world.balance(&world.treasury);

    world.open_season(params()).unwrap();

    let season = world.season(0);
    assert_eq!(season.id, 0);
    assert_eq!(season.params, params());
    assert_eq!(season.total_staked, 0);
    assert_eq!(season.total_weighted, 0);
    assert_eq!(season.weighted_stake_seconds, 0);
    // Accrual is measured from the start, not from the moment it was opened.
    assert_eq!(season.last_update_ts, START);
    assert!(!season.swept);

    assert_eq!(world.balance(&world.reward_vault(0)), params().reward_pool);
    assert_eq!(world.balance(&world.stake_vault(0)), 0);
    assert_eq!(world.balance(&world.treasury), before - params().reward_pool);
    assert_eq!(world.config().next_season, 1);
}

/// Principal and rewards are token accounts under different owners, so no
/// reward-side arithmetic can reach a staker's balance whatever it computes.
#[test]
fn the_two_vaults_are_distinct_accounts_under_different_authorities() {
    let mut world = World::new();
    world.open_season(params()).unwrap();

    assert_ne!(world.stake_vault(0), world.reward_vault(0));
    assert_ne!(world.season_address(0), world.rewards_authority(0));

    let stake_vault = world.svm.get_account(&world.stake_vault(0)).unwrap();
    let reward_vault = world.svm.get_account(&world.reward_vault(0)).unwrap();
    assert_eq!(
        spl_token::state::Account::unpack(&stake_vault.data)
            .unwrap()
            .owner,
        world.season_address(0)
    );
    assert_eq!(
        spl_token::state::Account::unpack(&reward_vault.data)
            .unwrap()
            .owner,
        world.rewards_authority(0)
    );
}

#[test]
fn seasons_are_allocated_sequentially_and_do_not_share_vaults() {
    let mut world = World::new();
    world.open_season(params()).unwrap();
    world.open_season(params()).unwrap();

    assert_eq!(world.config().next_season, 2);
    assert_eq!(world.season(1).id, 1);
    assert_ne!(world.reward_vault(0), world.reward_vault(1));
    assert_eq!(world.balance(&world.reward_vault(1)), params().reward_pool);
}

#[test]
fn open_season_refuses_invalid_parameters() {
    let mut world = World::new();

    let cases: [(SeasonParams, StakeError); 4] = [
        (
            SeasonParams {
                start_ts: NOW - 1,
                ..params()
            },
            StakeError::StartInThePast,
        ),
        (
            SeasonParams {
                end_ts: START,
                ..params()
            },
            StakeError::InvalidWindow,
        ),
        (
            SeasonParams {
                reward_pool: 0,
                ..params()
            },
            StakeError::InvalidParameters,
        ),
        (
            SeasonParams {
                max_multiplier_bps: ONE_X_BPS - 1,
                ..params()
            },
            StakeError::InvalidParameters,
        ),
    ];

    for (params, expected) in cases {
        assert_error(world.open_season(params), expected);
    }
}

#[test]
fn only_the_admin_can_open_a_season() {
    let mut world = World::new();
    let impostor = Keypair::new();
    world.svm.airdrop(&impostor.pubkey(), 10 * SOL).unwrap();

    assert_constraint(
        world.open_season_as(&impostor, params()),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
}

// -- update_season -----------------------------------------------------------

#[test]
fn update_season_rewrites_every_parameter_before_the_start() {
    let mut world = World::new();
    world.open_season(params()).unwrap();

    let revised = SeasonParams {
        start_ts: START + DAY,
        end_ts: START + DAY + 30 * DAY,
        reward_pool: 5 * MILLION,
        max_total_staked: 7 * MILLION,
        max_per_wallet: 3 * MILLION,
        max_apr_bps: 900,
        max_multiplier_bps: 3 * ONE_X_BPS,
        sweep_delay_seconds: SWEEP_DELAY + DAY as u32,
    };
    world.update_season(0, revised).unwrap();

    assert_eq!(world.season(0).params, revised);
    assert_eq!(world.season(0).last_update_ts, revised.start_ts);
}

/// One field of a `SeasonParams`, changed.
type Mutation = fn(SeasonParams) -> SeasonParams;

/// The freeze is a single assignment being refused, so no parameter can slip
/// past it — including one added after this test was written.
#[test]
fn every_parameter_is_frozen_once_the_season_starts() {
    let mutations: [(&str, Mutation); 8] = [
        ("start_ts", |p| SeasonParams {
            start_ts: p.start_ts + DAY,
            ..p
        }),
        ("end_ts", |p| SeasonParams {
            end_ts: p.end_ts + DAY,
            ..p
        }),
        ("reward_pool", |p| SeasonParams {
            reward_pool: p.reward_pool + MILLION,
            ..p
        }),
        ("max_total_staked", |p| SeasonParams {
            max_total_staked: p.max_total_staked + MILLION,
            ..p
        }),
        ("max_per_wallet", |p| SeasonParams {
            max_per_wallet: p.max_per_wallet + 1,
            ..p
        }),
        ("max_apr_bps", |p| SeasonParams {
            max_apr_bps: p.max_apr_bps + 1,
            ..p
        }),
        ("max_multiplier_bps", |p| SeasonParams {
            max_multiplier_bps: p.max_multiplier_bps + 1,
            ..p
        }),
        ("sweep_delay_seconds", |p| SeasonParams {
            sweep_delay_seconds: p.sweep_delay_seconds + 1,
            ..p
        }),
    ];

    for (field, mutate) in mutations {
        let mut world = World::new();
        world.open_season(params()).unwrap();
        world.warp_to(START);

        let result = world.update_season(0, mutate(params()));
        assert_error(result, StakeError::SeasonStarted);
        assert_eq!(world.season(0).params, params(), "{field} was not frozen");
    }
}

#[test]
fn a_season_is_frozen_even_when_nothing_was_ever_staked_in_it() {
    let mut world = World::new();
    world.open_season(params()).unwrap();
    world.warp_to(START + DURATION + 365 * DAY);

    assert_eq!(world.season(0).total_staked, 0);
    assert_error(
        world.update_season(0, params()),
        StakeError::SeasonStarted,
    );
}

#[test]
fn raising_the_pool_pulls_the_difference_into_the_vault() {
    let mut world = World::new();
    world.open_season(params()).unwrap();
    let treasury = world.balance(&world.treasury);

    let raised = SeasonParams {
        reward_pool: params().reward_pool + 3 * MILLION,
        ..params()
    };
    world.update_season(0, raised).unwrap();

    assert_eq!(world.balance(&world.reward_vault(0)), raised.reward_pool);
    assert_eq!(world.balance(&world.treasury), treasury - 3 * MILLION);
}

#[test]
fn lowering_the_pool_returns_the_difference() {
    let mut world = World::new();
    world.open_season(params()).unwrap();
    let treasury = world.balance(&world.treasury);

    let lowered = SeasonParams {
        reward_pool: params().reward_pool - MILLION,
        ..params()
    };
    world.update_season(0, lowered).unwrap();

    assert_eq!(world.balance(&world.reward_vault(0)), lowered.reward_pool);
    assert_eq!(world.balance(&world.treasury), treasury + MILLION);
}

/// Whatever else changes, the vault holds exactly the declared pool afterwards.
#[test]
fn the_reward_vault_always_matches_the_declared_pool() {
    let mut world = World::new();
    world.open_season(params()).unwrap();

    for pool in [9 * MILLION, MILLION, 4 * MILLION, 4 * MILLION, 1] {
        let revised = SeasonParams {
            reward_pool: pool,
            ..params()
        };
        world.update_season(0, revised).unwrap();
        assert_eq!(world.balance(&world.reward_vault(0)), pool);
        assert_eq!(world.season(0).params.reward_pool, pool);
    }
}

#[test]
fn update_season_refuses_a_start_moved_into_the_past() {
    let mut world = World::new();
    world.open_season(params()).unwrap();
    world.warp_to(START - DAY);

    let backdated = SeasonParams {
        start_ts: START - 2 * DAY,
        ..params()
    };
    assert_error(world.update_season(0, backdated), StakeError::StartInThePast);
}

#[test]
fn only_the_admin_can_update_a_season() {
    let mut world = World::new();
    world.open_season(params()).unwrap();

    let impostor = Keypair::new();
    world.svm.airdrop(&impostor.pubkey(), 10 * SOL).unwrap();

    assert_constraint(
        world.update_season_as(&impostor, 0, params()),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
}

// -- admin -------------------------------------------------------------------

#[test]
fn the_admin_can_pause_and_unpause() {
    let mut world = World::new();
    let admin = world.admin.insecure_clone();

    world
        .admin_ix(&admin, instruction::SetPaused { paused: true })
        .unwrap();
    assert!(world.config().paused);

    world
        .admin_ix(&admin, instruction::SetPaused { paused: false })
        .unwrap();
    assert!(!world.config().paused);
}

#[test]
fn the_admin_can_rotate_the_stake_signer() {
    let mut world = World::new();
    let admin = world.admin.insecure_clone();
    let replacement = Keypair::new();

    world
        .admin_ix(
            &admin,
            instruction::SetStakeSigner {
                stake_signer: replacement.pubkey(),
            },
        )
        .unwrap();

    assert_eq!(world.config().stake_signer, replacement.pubkey());
}

#[test]
fn a_handover_takes_both_steps() {
    let mut world = World::new();
    let admin = world.admin.insecure_clone();
    let successor = Keypair::new();
    world.svm.airdrop(&successor.pubkey(), 10 * SOL).unwrap();

    world.transfer_admin(&admin, Some(successor.pubkey())).unwrap();

    // Proposing changes nothing about who holds the key.
    assert_eq!(world.config().admin, admin.pubkey());
    assert_eq!(world.config().pending_admin, Some(successor.pubkey()));
    world
        .admin_ix(&admin, instruction::SetPaused { paused: true })
        .unwrap();

    world.accept_admin(&successor).unwrap();

    assert_eq!(world.config().admin, successor.pubkey());
    assert_eq!(world.config().pending_admin, None);
    world
        .admin_ix(&successor, instruction::SetPaused { paused: false })
        .unwrap();
    assert_constraint(
        world.admin_ix(&admin, instruction::SetPaused { paused: true }),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
}

/// The finding this instruction exists for. Milder here than in the claim
/// programs — `unstake` and `claim` are permissionless, so a key nobody holds
/// strands no user funds — but it would still brick `open_season` and
/// `sweep_unclaimed` permanently.
#[test]
fn a_mistyped_successor_never_takes_the_key() {
    let mut world = World::new();
    let admin = world.admin.insecure_clone();
    let unheld = Pubkey::new_unique();

    world.transfer_admin(&admin, Some(unheld)).unwrap();

    assert_eq!(world.config().admin, admin.pubkey());
    world.open_season(params()).unwrap();

    world.transfer_admin(&admin, None).unwrap();
    assert_eq!(world.config().pending_admin, None);
}

#[test]
fn only_the_pending_key_can_accept() {
    let mut world = World::new();
    let admin = world.admin.insecure_clone();
    let successor = Keypair::new();
    let impostor = Keypair::new();
    world.svm.airdrop(&successor.pubkey(), 10 * SOL).unwrap();
    world.svm.airdrop(&impostor.pubkey(), 10 * SOL).unwrap();

    // Nothing pending: even the current admin cannot accept.
    assert_error(world.accept_admin(&admin), StakeError::NotPendingAdmin);

    world.transfer_admin(&admin, Some(successor.pubkey())).unwrap();

    assert_error(world.accept_admin(&impostor), StakeError::NotPendingAdmin);
    assert_error(world.accept_admin(&admin), StakeError::NotPendingAdmin);
    assert_eq!(world.config().admin, admin.pubkey());

    world.accept_admin(&successor).unwrap();
    assert_eq!(world.config().admin, successor.pubkey());
}

#[test]
fn a_cancelled_or_superseded_proposal_cannot_be_accepted() {
    let mut world = World::new();
    let admin = world.admin.insecure_clone();
    let first = Keypair::new();
    let second = Keypair::new();
    world.svm.airdrop(&first.pubkey(), 10 * SOL).unwrap();
    world.svm.airdrop(&second.pubkey(), 10 * SOL).unwrap();

    world.transfer_admin(&admin, Some(first.pubkey())).unwrap();
    world.transfer_admin(&admin, None).unwrap();
    assert_error(world.accept_admin(&first), StakeError::NotPendingAdmin);

    world.transfer_admin(&admin, Some(first.pubkey())).unwrap();
    world.transfer_admin(&admin, Some(second.pubkey())).unwrap();
    assert_error(world.accept_admin(&first), StakeError::NotPendingAdmin);

    // Accepting is a one-shot: the slot clears, so nobody can take it back.
    world.accept_admin(&second).unwrap();
    assert_error(world.accept_admin(&first), StakeError::NotPendingAdmin);
}

#[test]
fn the_handover_emits_both_steps() {
    let mut world = World::new();
    let admin = world.admin.insecure_clone();
    let successor = Keypair::new();
    world.svm.airdrop(&successor.pubkey(), 10 * SOL).unwrap();

    let proposed = world.transfer_admin(&admin, Some(successor.pubkey()));
    let event: AdminTransferProposed = only_event(&proposed);
    assert_eq!(event.current, admin.pubkey());
    assert_eq!(event.pending, Some(successor.pubkey()));

    let accepted = world.accept_admin(&successor);
    let event: AdminTransferred = only_event(&accepted);
    assert_eq!(event.previous, admin.pubkey());
    assert_eq!(event.current, successor.pubkey());

    let cancelled = world.transfer_admin(&successor, None);
    let event: AdminTransferProposed = only_event(&cancelled);
    assert_eq!(event.pending, None);
    assert!(events_of::<AdminTransferred>(&cancelled).is_empty());
}

#[test]
fn admin_instructions_reject_everyone_else() {
    let mut world = World::new();
    let impostor = Keypair::new();
    world.svm.airdrop(&impostor.pubkey(), 10 * SOL).unwrap();

    for data in [
        instruction::SetPaused { paused: true }.data(),
        instruction::TransferAdmin {
            new_admin: Some(impostor.pubkey()),
        }
        .data(),
        instruction::SetStakeSigner {
            stake_signer: impostor.pubkey(),
        }
        .data(),
    ] {
        let ix = Instruction {
            program_id: PROGRAM,
            accounts: accounts::AdminOnly {
                admin: impostor.pubkey(),
                config: world.config,
            }
            .to_account_metas(None),
            data,
        };
        let impostor = impostor.insecure_clone();
        assert_constraint(
            world.send(&[ix], &impostor, &[&impostor]),
            anchor_lang::error::ErrorCode::ConstraintHasOne,
        );
    }
}

/// Pausing is a stake-side switch. It must not reach a season's parameters or
/// its vaults.
#[test]
fn pausing_does_not_block_opening_or_editing_a_season() {
    let mut world = World::new();
    let admin = world.admin.insecure_clone();
    world
        .admin_ix(&admin, instruction::SetPaused { paused: true })
        .unwrap();

    world.open_season(params()).unwrap();
    world
        .update_season(
            0,
            SeasonParams {
                reward_pool: MILLION,
                ..params()
            },
        )
        .unwrap();

    assert_eq!(world.balance(&world.reward_vault(0)), MILLION);
}

// -- events ------------------------------------------------------------------

/// Both season events carry the parameters whole, so an indexer never has to
/// read the account back to know what a season was opened or edited to.
#[test]
fn opening_and_editing_a_season_emit_the_parameters() {
    let mut world = World::new();

    let opened = world.open_season(params());
    let event: SeasonOpened = only_event(&opened);
    assert_eq!(event.id, 0);
    assert_eq!(event.params, params());

    let revised = SeasonParams {
        reward_pool: 5 * MILLION,
        ..params()
    };
    let updated = world.update_season(0, revised);
    let event: SeasonUpdated = only_event(&updated);
    assert_eq!(event.id, 0);
    assert_eq!(event.params, revised);
}

#[test]
fn the_admin_surface_is_observable() {
    let mut world = World::new();
    let replacement = Keypair::new();

    let admin = world.admin.insecure_clone();
    let previous = world.stake_signer.pubkey();

    let result = world.admin_ix(
        &admin,
        instruction::SetStakeSigner {
            stake_signer: replacement.pubkey(),
        },
    );
    let event: StakeSignerUpdated = only_event(&result);
    assert_eq!(event.previous, previous);
    assert_eq!(event.current, replacement.pubkey());

    let result = world.admin_ix(&admin, instruction::SetPaused { paused: true });
    let event: PausedSet = only_event(&result);
    assert!(event.paused);
}

// -- documented consequences -------------------------------------------------

/// The program places no constraint relating a new season's `start_ts` to a
/// previous season's `end_ts`, so two seasons can be live at once. A wallet
/// splitting its balance then accrues against both and is capped twice.
///
/// Prevented by `scripts/stake/season.ts`, not here. Pinned so the on-chain
/// behaviour is a recorded decision rather than an assumption.
#[test]
fn the_program_permits_overlapping_seasons() {
    let mut world = World::with_users(1);
    world.open_season(params()).unwrap();
    // Starts before season 0 closes.
    world
        .open_season(SeasonParams {
            start_ts: START + DAY,
            end_ts: END + DAY,
            ..params()
        })
        .unwrap();

    world.warp_to(START + DAY);
    let half = params().max_per_wallet / 2;
    world.stake(0, 0, half, ONE_X_BPS).unwrap();
    world.stake(0, 1, half, ONE_X_BPS).unwrap();

    // One wallet, two live positions, each accruing its own stake-seconds.
    world.warp_to(END);
    assert!(world.position(0, 0).amount > 0);
    assert!(world.position(1, 0).amount > 0);
    world.claim(0, 0).unwrap();
    assert!(world.user_balance(0) > 0);
}

/// The structural mitigation for the sticky multiplier: with the ceiling equal
/// to the floor, 1x is the only value `stake` accepts, so a compromised stake
/// signer cannot weight a position at all.
#[test]
fn a_one_x_ceiling_makes_one_x_the_only_reachable_multiplier() {
    let mut world = World::with_users(1);
    world
        .open_season(SeasonParams {
            max_multiplier_bps: ONE_X_BPS,
            ..params()
        })
        .unwrap();
    world.warp_to(START);

    assert_error(
        world.stake(0, 0, MILLION, ONE_X_BPS + 1),
        StakeError::MultiplierTooHigh,
    );
    assert_error(
        world.stake(0, 0, MILLION, ONE_X_BPS - 1),
        StakeError::MultiplierTooLow,
    );

    world.stake(0, 0, MILLION, ONE_X_BPS).unwrap();
    assert_eq!(world.position(0, 0).multiplier_bps, ONE_X_BPS);
}
