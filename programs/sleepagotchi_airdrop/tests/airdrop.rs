//! LiteSVM tests for `sleepagotchi_airdrop`.
//!
//! Compiles in the program binary from `target/deploy/`, so run `pnpm claim:test`,
//! which builds first. A bare `cargo test` on a clean tree fails to compile.

// litesvm's `TransactionResult` carries failure metadata in its `Err` by design.
#![allow(clippy::result_large_err)]

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, program_pack::Pack, system_instruction},
    AccountDeserialize, AnchorDeserialize, InstructionData, ToAccountMetas,
};
use anchor_spl::{
    associated_token::{
        get_associated_token_address, spl_associated_token_account, ID as ATA_PROGRAM,
    },
    token::spl_token,
};
use litesvm::{types::TransactionResult, LiteSVM};
use sleepagotchi_airdrop::{
    accounts,
    error::AirdropError,
    events::{
        AdminTransferProposed, AdminTransferred, Claimed, PausedSet, RootUpdated, WindowUpdated,
        Withdrawn,
    },
    instruction,
    merkle::tree,
    state::Config,
    CONFIG_SEED, RECEIPT_SEED,
};
use solana_clock::Clock;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

const PROGRAM: Pubkey = sleepagotchi_airdrop::ID;
const SOL: u64 = 1_000_000_000;
const DECIMALS: u8 = 9;
/// One whole $SLEEP in base units.
const UNIT: u64 = 1_000_000_000;
/// Claim window used by `World`, with room to warp either side of it.
const START: i64 = 1_000;
const END: i64 = 2_000;
const MID: i64 = 1_500;

struct World {
    svm: LiteSVM,
    admin: Keypair,
    mint: Pubkey,
    config: Pubkey,
    vault: Pubkey,
    users: Vec<Keypair>,
    allocations: Vec<(Pubkey, u64)>,
    levels: Vec<Vec<[u8; 32]>>,
}

impl World {
    /// `n` claimants with allocations of 1, 2, … `n` whole tokens, a vault holding
    /// `vault_tokens`, and the clock inside the claim window.
    fn new(n: usize, vault_tokens: u64) -> Self {
        // History off so a resubmitted transaction reaches the program instead of
        // being deduplicated by the runtime.
        let mut svm = LiteSVM::new().with_transaction_history(0);
        svm.add_program(
            PROGRAM,
            include_bytes!("../../../target/deploy/sleepagotchi_airdrop.so"),
        )
        .unwrap();

        let admin = Keypair::new();
        svm.airdrop(&admin.pubkey(), 1_000 * SOL).unwrap();

        let mint = Keypair::new();
        let (config, _) = Pubkey::find_program_address(&[CONFIG_SEED], &PROGRAM);
        let vault = get_associated_token_address(&config, &mint.pubkey());

        let users: Vec<Keypair> = (0..n).map(|_| Keypair::new()).collect();
        for user in &users {
            svm.airdrop(&user.pubkey(), SOL).unwrap();
        }

        let allocations: Vec<(Pubkey, u64)> = users
            .iter()
            .enumerate()
            .map(|(i, user)| (user.pubkey(), (i as u64 + 1) * UNIT))
            .collect();
        let levels = tree::levels(&allocations);

        let mut world = Self {
            svm,
            admin,
            mint: mint.pubkey(),
            config,
            vault,
            users,
            allocations,
            levels,
        };

        world.create_mint(&mint);
        world.initialize(tree::root(&world.levels.clone()), START, END);
        world.fund_vault(vault_tokens * UNIT);
        world.warp_to(MID);
        world
    }

    fn warp_to(&mut self, unix_timestamp: i64) {
        let mut clock = self.svm.get_sysvar::<Clock>();
        clock.unix_timestamp = unix_timestamp;
        self.svm.set_sysvar::<Clock>(&clock);
    }

    fn create_mint(&mut self, mint: &Keypair) {
        let admin = self.admin.insecure_clone();
        let ixs = [
            system_instruction::create_account(
                &admin.pubkey(),
                &mint.pubkey(),
                10 * SOL / 1_000,
                spl_token::state::Mint::LEN as u64,
                &spl_token::ID,
            ),
            spl_token::instruction::initialize_mint2(
                &spl_token::ID,
                &mint.pubkey(),
                &admin.pubkey(),
                None,
                DECIMALS,
            )
            .unwrap(),
        ];

        self.send(&ixs, &admin, &[&admin, mint]).unwrap();
    }

    fn initialize(&mut self, root: [u8; 32], start_ts: i64, end_ts: i64) {
        let admin = self.admin.insecure_clone();
        let ix = self.ix(
            accounts::Initialize {
                admin: admin.pubkey(),
                config: self.config,
                mint: self.mint,
                vault: self.vault,
                token_program: spl_token::ID,
                associated_token_program: ATA_PROGRAM,
                system_program: anchor_lang::system_program::ID,
            },
            instruction::Initialize {
                root,
                start_ts,
                end_ts,
            },
        );

        self.send(&[ix], &admin, &[&admin]).unwrap();
    }

    /// Stands in for the Squads funding transfer.
    fn fund_vault(&mut self, amount: u64) {
        if amount == 0 {
            return;
        }

        let admin = self.admin.insecure_clone();
        let ix = spl_token::instruction::mint_to(
            &spl_token::ID,
            &self.mint,
            &self.vault,
            &admin.pubkey(),
            &[],
            amount,
        )
        .unwrap();

        self.send(&[ix], &admin, &[&admin]).unwrap();
    }

    fn ix(&self, accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
        Instruction {
            program_id: PROGRAM,
            accounts: accounts.to_account_metas(None),
            data: data.data(),
        }
    }

    fn send(
        &mut self,
        ixs: &[Instruction],
        payer: &Keypair,
        signers: &[&Keypair],
    ) -> TransactionResult {
        let message = Message::new(ixs, Some(&payer.pubkey()));
        let tx = Transaction::new(signers, message, self.svm.latest_blockhash());

        self.svm.send_transaction(tx)
    }

    fn receipt(&self, user: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[RECEIPT_SEED, user.as_ref()], &PROGRAM).0
    }

    /// Creates `user`'s ATA if missing, then claims — the two instructions the app
    /// will send.
    fn claim_as(
        &mut self,
        signer: &Keypair,
        leaf_user: Pubkey,
        amount: u64,
        proof: Vec<[u8; 32]>,
    ) -> TransactionResult {
        self.claim_to(signer, leaf_user, amount, proof, None)
    }

    fn claim_to(
        &mut self,
        signer: &Keypair,
        leaf_user: Pubkey,
        amount: u64,
        proof: Vec<[u8; 32]>,
        destination: Option<Pubkey>,
    ) -> TransactionResult {
        let destination =
            destination.unwrap_or_else(|| get_associated_token_address(&leaf_user, &self.mint));

        let mut ixs = vec![];
        if self.svm.get_account(&destination).is_none() {
            ixs.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    &signer.pubkey(),
                    &leaf_user,
                    &self.mint,
                    &spl_token::ID,
                ),
            );
        }

        ixs.push(self.ix(
            accounts::Claim {
                user: leaf_user,
                config: self.config,
                receipt: self.receipt(&leaf_user),
                mint: self.mint,
                vault: self.vault,
                destination,
                token_program: spl_token::ID,
                system_program: anchor_lang::system_program::ID,
            },
            instruction::Claim { amount, proof },
        ));

        let signer = signer.insecure_clone();
        self.send(&ixs, &signer, &[&signer])
    }

    fn claim(&mut self, index: usize) -> TransactionResult {
        let user = self.users[index].insecure_clone();
        let amount = self.allocations[index].1;
        let proof = tree::proof(&self.levels, index);

        self.claim_as(&user, user.pubkey(), amount, proof)
    }

    fn set_paused(&mut self, paused: bool) -> TransactionResult {
        let admin = self.admin.insecure_clone();
        let ix = self.ix(
            accounts::AdminOnly {
                admin: admin.pubkey(),
                config: self.config,
            },
            instruction::SetPaused { paused },
        );

        self.send(&[ix], &admin, &[&admin])
    }

    fn set_window(&mut self, signer: &Keypair, start_ts: i64, end_ts: i64) -> TransactionResult {
        let ix = self.ix(
            accounts::AdminOnly {
                admin: signer.pubkey(),
                config: self.config,
            },
            instruction::SetWindow { start_ts, end_ts },
        );

        let signer = signer.insecure_clone();
        self.send(&[ix], &signer, &[&signer])
    }

    fn set_root(&mut self, signer: &Keypair, root: [u8; 32]) -> TransactionResult {
        let ix = self.ix(
            accounts::AdminOnly {
                admin: signer.pubkey(),
                config: self.config,
            },
            instruction::SetRoot { root },
        );

        let signer = signer.insecure_clone();
        self.send(&[ix], &signer, &[&signer])
    }

    fn transfer_admin(&mut self, signer: &Keypair, new_admin: Option<Pubkey>) -> TransactionResult {
        let ix = self.ix(
            accounts::AdminOnly {
                admin: signer.pubkey(),
                config: self.config,
            },
            instruction::TransferAdmin { new_admin },
        );

        let signer = signer.insecure_clone();
        self.send(&[ix], &signer, &[&signer])
    }

    fn accept_admin(&mut self, signer: &Keypair) -> TransactionResult {
        let ix = self.ix(
            accounts::AcceptAdmin {
                pending_admin: signer.pubkey(),
                config: self.config,
            },
            instruction::AcceptAdmin {},
        );

        let signer = signer.insecure_clone();
        self.send(&[ix], &signer, &[&signer])
    }

    fn config(&self) -> Config {
        let account = self.svm.get_account(&self.config).unwrap();
        Config::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    fn token_balance(&self, address: &Pubkey) -> u64 {
        self.svm
            .get_account(address)
            .map(|account| {
                spl_token::state::Account::unpack(&account.data)
                    .unwrap()
                    .amount
            })
            .unwrap_or_default()
    }

    fn balance_of(&self, index: usize) -> u64 {
        self.token_balance(&get_associated_token_address(
            &self.users[index].pubkey(),
            &self.mint,
        ))
    }
}

/// Decodes every Anchor event of type `E` out of a successful transaction's logs.
///
/// `emit!` base64-encodes `discriminator || borsh(event)` behind a
/// `Program data:` line, so this is the same path an off-chain listener takes —
/// which is what makes it a test of the monitoring surface rather than of the
/// macro.
fn events_of<E: anchor_lang::Event + anchor_lang::Discriminator + AnchorDeserialize>(
    result: &TransactionResult,
) -> Vec<E> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let logs = &result.as_ref().expect("expected success").logs;

    logs.iter()
        .filter_map(|line| line.strip_prefix("Program data: "))
        .filter_map(|payload| STANDARD.decode(payload).ok())
        .filter_map(|bytes| {
            let (discriminator, mut body) = bytes.split_at(E::DISCRIMINATOR.len());
            (discriminator == E::DISCRIMINATOR).then(|| E::deserialize(&mut body).unwrap())
        })
        .collect()
}

fn only_event<E: anchor_lang::Event + anchor_lang::Discriminator + AnchorDeserialize>(
    result: &TransactionResult,
) -> E {
    let mut found = events_of::<E>(result);

    assert_eq!(found.len(), 1, "expected exactly one event");
    found.remove(0)
}

fn assert_code(result: TransactionResult, code: u32) {
    let failure = result.expect_err("expected the transaction to fail");

    assert!(
        format!("{:?}", failure.err).contains(&format!("Custom({code})")),
        "expected Custom({code}), got {:?}\nlogs: {:#?}",
        failure.err,
        failure.meta.logs,
    );
}

fn assert_error(result: TransactionResult, expected: AirdropError) {
    // `#[error_code]` numbers variants from zero and adds the offset on conversion.
    assert_code(
        result,
        expected as u32 + anchor_lang::error::ERROR_CODE_OFFSET,
    );
}

/// Anchor's own constraint codes already carry their absolute number.
fn assert_constraint(result: TransactionResult, expected: anchor_lang::error::ErrorCode) {
    assert_code(result, expected as u32);
}

/// Fails inside the receipt's `init` CPI, so the error is the system program's
/// `AccountAlreadyInUse` rather than one of ours.
fn assert_receipt_exists(result: TransactionResult) {
    assert_code(result, 0);
}

// -- happy paths -------------------------------------------------------------

#[test]
fn claims_its_allocation() {
    let mut world = World::new(8, 100);

    world.claim(3).unwrap();

    assert_eq!(world.balance_of(3), 4 * UNIT);
    assert_eq!(world.token_balance(&world.vault), 96 * UNIT);
}

#[test]
fn every_claimant_can_claim() {
    let mut world = World::new(7, 100);

    for i in 0..7 {
        world.claim(i).unwrap();
        assert_eq!(world.balance_of(i), (i as u64 + 1) * UNIT);
    }
}

/// Odd tree sizes promote an unpaired node, which is the shape most likely to be
/// built one way and verified another.
#[test]
fn odd_sized_trees_claim() {
    for n in [1, 3, 5, 9] {
        let mut world = World::new(n, 100);

        for i in 0..n {
            world.claim(i).unwrap();
        }
    }
}

#[test]
fn rejects_a_claim_before_the_window_opens() {
    let mut world = World::new(4, 100);
    world.warp_to(START - 1);

    assert_error(world.claim(0), AirdropError::NotStarted);
}

#[test]
fn rejects_a_claim_after_the_window_closes() {
    let mut world = World::new(4, 100);
    world.warp_to(END);

    assert_error(world.claim(0), AirdropError::Ended);
}

/// Half-open `[start, end)`: open on the first second, closed on the last.
#[test]
fn the_window_is_half_open() {
    let mut world = World::new(4, 100);

    world.warp_to(START);
    world.claim(0).unwrap();

    world.warp_to(END - 1);
    world.claim(1).unwrap();

    world.warp_to(END);
    assert_error(world.claim(2), AirdropError::Ended);
}

#[test]
fn admin_can_extend_a_closed_window() {
    let mut world = World::new(4, 100);
    world.warp_to(END);
    assert_error(world.claim(0), AirdropError::Ended);

    world
        .set_window(&world.admin.insecure_clone(), START, END + 500)
        .unwrap();
    world.claim(0).unwrap();

    assert_eq!(world.balance_of(0), UNIT);
    assert_eq!(world.config().end_ts, END + 500);
}

#[test]
fn rejects_an_inverted_window() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();

    assert_error(
        world.set_window(&admin, END, START),
        AirdropError::InvalidWindow,
    );
    assert_error(
        world.set_window(&admin, START, START),
        AirdropError::InvalidWindow,
    );
    assert_eq!(world.config().end_ts, END);
}

#[test]
fn rejects_set_window_from_a_non_admin() {
    let mut world = World::new(4, 100);
    let impostor = world.users[0].insecure_clone();

    assert_constraint(
        world.set_window(&impostor, START, END + 500),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
    assert_eq!(world.config().end_ts, END);
}

/// The kill switch is independent of the schedule.
#[test]
fn paused_blocks_a_claim_inside_the_window() {
    let mut world = World::new(4, 100);
    world.set_paused(true).unwrap();

    assert_error(world.claim(0), AirdropError::Paused);

    world.set_paused(false).unwrap();
    world.claim(0).unwrap();
}

/// A wallet the first root missed can claim once the root is rotated, and the
/// wallets in both trees are unaffected.
#[test]
fn claims_after_a_root_rotation() {
    let mut world = World::new(4, 100);
    world.claim(0).unwrap();

    let latecomer = Keypair::new();
    world.svm.airdrop(&latecomer.pubkey(), SOL).unwrap();

    let mut allocations = world.allocations.clone();
    allocations.push((latecomer.pubkey(), 42 * UNIT));
    let levels = tree::levels(&allocations);

    let admin = world.admin.insecure_clone();
    world.set_root(&admin, tree::root(&levels)).unwrap();
    world.levels = levels;
    world.allocations = allocations;

    let proof = tree::proof(&world.levels, 4);
    world
        .claim_as(&latecomer, latecomer.pubkey(), 42 * UNIT, proof)
        .unwrap();

    assert_eq!(
        world.token_balance(&get_associated_token_address(
            &latecomer.pubkey(),
            &world.mint
        )),
        42 * UNIT
    );

    // A wallet present in both trees still claims against the new root.
    world.claim(1).unwrap();
    assert_eq!(world.balance_of(1), 2 * UNIT);
}

#[test]
fn admin_can_withdraw_the_vault() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let destination = get_associated_token_address(&admin.pubkey(), &world.mint);

    let create = spl_associated_token_account::instruction::create_associated_token_account(
        &admin.pubkey(),
        &admin.pubkey(),
        &world.mint,
        &spl_token::ID,
    );
    let withdraw = world.ix(
        accounts::Withdraw {
            admin: admin.pubkey(),
            config: world.config,
            mint: world.mint,
            vault: world.vault,
            destination,
            token_program: spl_token::ID,
        },
        instruction::Withdraw { amount: 100 * UNIT },
    );

    world.send(&[create, withdraw], &admin, &[&admin]).unwrap();

    assert_eq!(world.token_balance(&destination), 100 * UNIT);
    assert_eq!(world.token_balance(&world.vault), 0);
}

// -- claims that must fail ---------------------------------------------------

#[test]
fn rejects_a_proof_for_a_different_user() {
    let mut world = World::new(8, 100);
    let user = world.users[2].insecure_clone();
    let stolen = tree::proof(&world.levels, 5);

    assert_error(
        world.claim_as(&user, user.pubkey(), world.allocations[5].1, stolen),
        AirdropError::InvalidProof,
    );
}

#[test]
fn rejects_a_proof_for_a_different_amount() {
    let mut world = World::new(8, 100);
    let user = world.users[2].insecure_clone();
    let proof = tree::proof(&world.levels, 2);

    assert_error(
        world.claim_as(&user, user.pubkey(), 99 * UNIT, proof),
        AirdropError::InvalidProof,
    );
}

#[test]
fn rejects_a_proof_against_a_stale_root() {
    let mut world = World::new(8, 100);
    let stale = tree::proof(&world.levels, 2);

    // Rotate to a tree the claimant is not in.
    let others: Vec<(Pubkey, u64)> = (0..4).map(|i| (Pubkey::new_unique(), i * UNIT)).collect();
    let admin = world.admin.insecure_clone();
    world
        .set_root(&admin, tree::root(&tree::levels(&others)))
        .unwrap();

    let user = world.users[2].insecure_clone();
    assert_error(
        world.claim_as(&user, user.pubkey(), world.allocations[2].1, stale),
        AirdropError::InvalidProof,
    );
}

#[test]
fn rejects_a_second_claim_by_the_same_wallet() {
    let mut world = World::new(8, 100);
    world.claim(3).unwrap();

    // The receipt exists now, so `init` fails inside the system program rather
    // than raising one of our errors.
    assert_receipt_exists(world.claim(3));
    assert_eq!(world.balance_of(3), 4 * UNIT);
}

/// Rotating the root does not resurrect a spent claim: the receipt is seeded by
/// wallet alone and never references the root.
#[test]
fn a_new_root_does_not_reopen_a_spent_claim() {
    let mut world = World::new(4, 100);
    world.claim(1).unwrap();

    let admin = world.admin.insecure_clone();
    let levels = tree::levels(&world.allocations.clone());
    world.set_root(&admin, tree::root(&levels)).unwrap();

    assert_receipt_exists(world.claim(1));
    assert_eq!(world.balance_of(1), 2 * UNIT);
}

#[test]
fn rejects_a_claim_the_vault_cannot_cover() {
    let mut world = World::new(8, 1);

    assert_error(world.claim(7), AirdropError::InsufficientVault);
}

#[test]
fn rejects_an_over_long_proof() {
    let mut world = World::new(8, 100);
    let user = world.users[2].insecure_clone();
    let bloated = vec![[0u8; 32]; sleepagotchi_airdrop::merkle::MAX_PROOF_LEN + 1];

    assert_error(
        world.claim_as(&user, user.pubkey(), world.allocations[2].1, bloated),
        AirdropError::InvalidProof,
    );
}

/// The `associated_token::authority = user` constraint is the only thing stopping
/// a claimant redirecting their allocation, so it gets its own test.
#[test]
fn rejects_a_destination_that_is_not_the_claimants_ata() {
    let mut world = World::new(8, 100);
    let user = world.users[2].insecure_clone();
    let proof = tree::proof(&world.levels, 2);
    let elsewhere = get_associated_token_address(&world.users[5].pubkey(), &world.mint);

    // Give the target ATA an existence so the failure is the constraint, not a
    // missing account.
    let admin = world.admin.insecure_clone();
    let create = spl_associated_token_account::instruction::create_associated_token_account(
        &admin.pubkey(),
        &world.users[5].pubkey(),
        &world.mint,
        &spl_token::ID,
    );
    world.send(&[create], &admin, &[&admin]).unwrap();

    assert_constraint(
        world.claim_to(
            &user,
            user.pubkey(),
            world.allocations[2].1,
            proof,
            Some(elsewhere),
        ),
        anchor_lang::error::ErrorCode::ConstraintTokenOwner,
    );
    assert_eq!(world.token_balance(&elsewhere), 0);
}

// -- admin gating ------------------------------------------------------------

#[test]
fn rejects_set_root_from_a_non_admin() {
    let mut world = World::new(4, 100);
    let impostor = world.users[0].insecure_clone();
    let before = world.config().root;

    assert_constraint(
        world.set_root(&impostor, [7; 32]),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
    assert_eq!(world.config().root, before);
}

#[test]
fn rejects_set_paused_and_transfer_admin_from_a_non_admin() {
    let mut world = World::new(4, 100);
    let impostor = world.users[0].insecure_clone();

    let pause = world.ix(
        accounts::AdminOnly {
            admin: impostor.pubkey(),
            config: world.config,
        },
        instruction::SetPaused { paused: true },
    );
    assert_constraint(
        world.send(&[pause], &impostor, &[&impostor]),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
    assert!(!world.config().paused);

    assert_constraint(
        world.transfer_admin(&impostor, Some(impostor.pubkey())),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
    assert_eq!(world.config().admin, world.admin.pubkey());
    assert_eq!(world.config().pending_admin, None);
}

// -- events ------------------------------------------------------------------

#[test]
fn a_claim_emits_the_amount_and_the_claimant() {
    let mut world = World::new(8, 100);
    let user = world.users[3].pubkey();

    let result = world.claim(3);

    let event: Claimed = only_event(&result);
    assert_eq!(event.user, user);
    assert_eq!(event.amount, 4 * UNIT);
}

/// A rotation carries both sides, so a listener can tell which root was replaced
/// without having read the account beforehand.
#[test]
fn a_root_rotation_emits_both_roots() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let before = world.config().root;

    let result = world.set_root(&admin, [9; 32]);

    let event: RootUpdated = only_event(&result);
    assert_eq!(event.previous, before);
    assert_eq!(event.current, [9; 32]);
}

#[test]
fn the_admin_handover_emits_both_steps() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let successor = Keypair::new();
    world.svm.airdrop(&successor.pubkey(), SOL).unwrap();

    let proposed = world.transfer_admin(&admin, Some(successor.pubkey()));
    let event: AdminTransferProposed = only_event(&proposed);
    assert_eq!(event.current, admin.pubkey());
    assert_eq!(event.pending, Some(successor.pubkey()));

    let accepted = world.accept_admin(&successor);
    let event: AdminTransferred = only_event(&accepted);
    assert_eq!(event.previous, admin.pubkey());
    assert_eq!(event.current, successor.pubkey());

    // Cancellation is observable too, and is not mistaken for a completed handover.
    let cancelled = world.transfer_admin(&successor, None);
    let event: AdminTransferProposed = only_event(&cancelled);
    assert_eq!(event.pending, None);
    assert!(events_of::<AdminTransferred>(&cancelled).is_empty());
}

#[test]
fn the_window_the_pause_and_a_withdrawal_are_all_observable() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();

    let result = world.set_window(&admin, START, END + 500);
    let event: WindowUpdated = only_event(&result);
    assert_eq!((event.start_ts, event.end_ts), (START, END + 500));

    let result = world.set_paused(true);
    let event: PausedSet = only_event(&result);
    assert!(event.paused);

    let destination = get_associated_token_address(&admin.pubkey(), &world.mint);
    let create = spl_associated_token_account::instruction::create_associated_token_account(
        &admin.pubkey(),
        &admin.pubkey(),
        &world.mint,
        &spl_token::ID,
    );
    let withdraw = world.ix(
        accounts::Withdraw {
            admin: admin.pubkey(),
            config: world.config,
            mint: world.mint,
            vault: world.vault,
            destination,
            token_program: spl_token::ID,
        },
        instruction::Withdraw { amount: 40 * UNIT },
    );
    let result = world.send(&[create, withdraw], &admin, &[&admin]);

    let event: Withdrawn = only_event(&result);
    assert_eq!(event.amount, 40 * UNIT);
    assert_eq!(event.destination, destination);
}

/// A failed transaction emits nothing — the log is a record of what happened,
/// not of what was attempted.
#[test]
fn a_refused_claim_emits_nothing() {
    let mut world = World::new(4, 100);
    world.set_paused(true).unwrap();

    let result = world.claim(0);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .meta
        .logs
        .iter()
        .all(|line| !line.starts_with("Program data: ")));
}

// -- admin handover ----------------------------------------------------------

#[test]
fn a_handover_takes_both_steps() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let successor = Keypair::new();
    world.svm.airdrop(&successor.pubkey(), SOL).unwrap();

    world
        .transfer_admin(&admin, Some(successor.pubkey()))
        .unwrap();

    // Proposing changes nothing about who holds the key.
    assert_eq!(world.config().admin, admin.pubkey());
    assert_eq!(world.config().pending_admin, Some(successor.pubkey()));
    world.set_root(&admin, [1; 32]).unwrap();

    world.accept_admin(&successor).unwrap();

    assert_eq!(world.config().admin, successor.pubkey());
    assert_eq!(world.config().pending_admin, None);
    world.set_root(&successor, [2; 32]).unwrap();
    assert_constraint(
        world.set_root(&admin, [3; 32]),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
}

/// The finding this instruction exists for: a key nobody holds can be proposed,
/// and never becomes the admin because it cannot sign the second step.
#[test]
fn a_mistyped_successor_never_takes_the_key() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let unheld = Pubkey::new_unique();

    world.transfer_admin(&admin, Some(unheld)).unwrap();

    assert_eq!(world.config().admin, admin.pubkey());
    world.set_root(&admin, [1; 32]).unwrap();

    // And it is recoverable — the proposal is cancelled and the vault stays reachable.
    world.transfer_admin(&admin, None).unwrap();
    assert_eq!(world.config().pending_admin, None);
}

#[test]
fn only_the_pending_key_can_accept() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let successor = Keypair::new();
    let impostor = world.users[0].insecure_clone();
    world.svm.airdrop(&successor.pubkey(), SOL).unwrap();

    // Nothing pending: even the current admin cannot accept.
    assert_error(world.accept_admin(&admin), AirdropError::NotPendingAdmin);

    world
        .transfer_admin(&admin, Some(successor.pubkey()))
        .unwrap();

    assert_error(world.accept_admin(&impostor), AirdropError::NotPendingAdmin);
    assert_error(world.accept_admin(&admin), AirdropError::NotPendingAdmin);
    assert_eq!(world.config().admin, admin.pubkey());

    world.accept_admin(&successor).unwrap();
    assert_eq!(world.config().admin, successor.pubkey());
}

#[test]
fn a_cancelled_handover_cannot_be_accepted() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let successor = Keypair::new();
    world.svm.airdrop(&successor.pubkey(), SOL).unwrap();

    world
        .transfer_admin(&admin, Some(successor.pubkey()))
        .unwrap();
    world.transfer_admin(&admin, None).unwrap();

    assert_error(
        world.accept_admin(&successor),
        AirdropError::NotPendingAdmin,
    );
    assert_eq!(world.config().admin, admin.pubkey());
}

/// Accepting is a one-shot: the pending slot is cleared, so a superseded key
/// cannot take the admin back later.
#[test]
fn a_superseded_proposal_cannot_be_accepted() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let first = Keypair::new();
    let second = Keypair::new();
    world.svm.airdrop(&first.pubkey(), SOL).unwrap();
    world.svm.airdrop(&second.pubkey(), SOL).unwrap();

    world.transfer_admin(&admin, Some(first.pubkey())).unwrap();
    world.transfer_admin(&admin, Some(second.pubkey())).unwrap();

    assert_error(world.accept_admin(&first), AirdropError::NotPendingAdmin);

    world.accept_admin(&second).unwrap();
    assert_eq!(world.config().admin, second.pubkey());
    assert_error(world.accept_admin(&first), AirdropError::NotPendingAdmin);
}

#[test]
fn rejects_withdraw_from_a_non_admin() {
    let mut world = World::new(4, 100);
    let impostor = world.users[0].insecure_clone();
    let destination = get_associated_token_address(&impostor.pubkey(), &world.mint);

    let create = spl_associated_token_account::instruction::create_associated_token_account(
        &impostor.pubkey(),
        &impostor.pubkey(),
        &world.mint,
        &spl_token::ID,
    );
    let withdraw = world.ix(
        accounts::Withdraw {
            admin: impostor.pubkey(),
            config: world.config,
            mint: world.mint,
            vault: world.vault,
            destination,
            token_program: spl_token::ID,
        },
        instruction::Withdraw { amount: 100 * UNIT },
    );

    assert_constraint(
        world.send(&[create, withdraw], &impostor, &[&impostor]),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
    assert_eq!(world.token_balance(&world.vault), 100 * UNIT);
}

// -- documented consequences -------------------------------------------------

/// `withdraw` has no timing guard. It is callable mid-window, with claimants
/// holding valid proofs, and every claim after it fails `InsufficientVault`.
///
/// Pinned rather than assumed: the behaviour was untested, which is how the
/// scope document came to describe `withdraw` as a post-`end_ts` reclaim path
/// that the program does not actually restrict.
#[test]
fn withdraw_is_unrestricted_while_the_window_is_open() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let destination = get_associated_token_address(&admin.pubkey(), &world.mint);

    // Mid-window, and one wallet has already been paid.
    world.claim(0).unwrap();
    assert!(world.config().start_ts <= MID && MID < world.config().end_ts);

    let create = spl_associated_token_account::instruction::create_associated_token_account(
        &admin.pubkey(),
        &admin.pubkey(),
        &world.mint,
        &spl_token::ID,
    );
    let withdraw = world.ix(
        accounts::Withdraw {
            admin: admin.pubkey(),
            config: world.config,
            mint: world.mint,
            vault: world.vault,
            destination,
            token_program: spl_token::ID,
        },
        instruction::Withdraw {
            amount: 99 * UNIT,
        },
    );
    world.send(&[create, withdraw], &admin, &[&admin]).unwrap();

    // The window is still open and the proof is still valid; there is simply
    // nothing left to pay it with.
    assert!(!world.config().paused);
    assert_error(world.claim(1), AirdropError::InsufficientVault);
}

/// A wallet appearing twice in the tree can only ever claim one of its leaves:
/// the receipt is seeded by wallet alone and does not record which allocation
/// it came from, so the second is unclaimable forever.
///
/// Uniqueness is enforced where the tree is built, off chain. This pins the
/// on-chain consequence of getting that wrong.
#[test]
fn a_wallet_with_two_leaves_can_only_claim_one() {
    let mut world = World::new(4, 100);
    let duplicate = world.users[0].insecure_clone();

    // The same wallet again, at a different amount, appended to the tree.
    let mut allocations = world.allocations.clone();
    allocations.push((duplicate.pubkey(), 50 * UNIT));
    let levels = tree::levels(&allocations);

    let admin = world.admin.insecure_clone();
    world.set_root(&admin, tree::root(&levels)).unwrap();
    world.levels = levels;
    world.allocations = allocations;

    // Either leaf verifies against the root on its own.
    let first = tree::proof(&world.levels, 0);
    let second = tree::proof(&world.levels, 4);

    world
        .claim_as(&duplicate, duplicate.pubkey(), UNIT, first)
        .unwrap();
    assert_eq!(world.balance_of(0), UNIT);

    // The second is refused by the receipt, not by the proof.
    assert_receipt_exists(world.claim_as(&duplicate, duplicate.pubkey(), 50 * UNIT, second));
    assert_eq!(world.balance_of(0), UNIT);
}
