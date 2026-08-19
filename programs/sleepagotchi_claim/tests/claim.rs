//! LiteSVM tests for `sleepagotchi_claim`.
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
use sleepagotchi_claim::{
    accounts,
    error::ClaimError,
    events::{
        AdminTransferProposed, AdminTransferred, ClaimSignerUpdated, Claimed, PausedSet, Withdrawn,
    },
    instruction,
    state::{Config, Receipt},
    CONFIG_SEED, RECEIPT_SEED,
};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

const PROGRAM: Pubkey = sleepagotchi_claim::ID;
const SOL: u64 = 1_000_000_000;
const DECIMALS: u8 = 9;
/// One whole $SLEEP in base units.
const UNIT: u64 = 1_000_000_000;

struct World {
    svm: LiteSVM,
    admin: Keypair,
    claim_signer: Keypair,
    mint: Pubkey,
    config: Pubkey,
    vault: Pubkey,
    users: Vec<Keypair>,
}

impl World {
    /// `n` users, a vault holding `vault_tokens`, initialized and unpaused.
    fn new(n: usize, vault_tokens: u64) -> Self {
        // History off so a resubmitted transaction reaches the program instead of
        // being deduplicated by the runtime.
        let mut svm = LiteSVM::new().with_transaction_history(0);
        svm.add_program(
            PROGRAM,
            include_bytes!("../../../target/deploy/sleepagotchi_claim.so"),
        )
        .unwrap();

        let admin = Keypair::new();
        svm.airdrop(&admin.pubkey(), 1_000 * SOL).unwrap();

        // Never funded — the whole point of the design.
        let claim_signer = Keypair::new();

        let mint = Keypair::new();
        let (config, _) = Pubkey::find_program_address(&[CONFIG_SEED], &PROGRAM);
        let vault = get_associated_token_address(&config, &mint.pubkey());

        let users: Vec<Keypair> = (0..n).map(|_| Keypair::new()).collect();
        for user in &users {
            svm.airdrop(&user.pubkey(), SOL).unwrap();
        }

        let mut world = Self {
            svm,
            admin,
            claim_signer,
            mint: mint.pubkey(),
            config,
            vault,
            users,
        };

        world.create_mint(&mint);
        world.initialize();
        world.fund_vault(vault_tokens * UNIT);
        world
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

    fn initialize(&mut self) {
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
                claim_signer: self.claim_signer.pubkey(),
            },
        );

        self.send(&[ix], &admin, &[&admin]).unwrap();
    }

    /// Stands in for the Squads tranche transfer.
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

    /// Signs with only `signers`, leaving any other required signature blank.
    fn send_partially_signed(
        &mut self,
        ixs: &[Instruction],
        payer: &Keypair,
        signers: &[&Keypair],
    ) -> TransactionResult {
        let message = Message::new(ixs, Some(&payer.pubkey()));
        let mut tx = Transaction::new_unsigned(message);
        tx.partial_sign(signers, self.svm.latest_blockhash());

        self.svm.send_transaction(tx)
    }

    fn receipt_address(&self, user: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(&[RECEIPT_SEED, user.as_ref()], &PROGRAM).0
    }

    fn claim_ix(
        &self,
        user: &Pubkey,
        signer: &Pubkey,
        total: u64,
        destination: Pubkey,
    ) -> Instruction {
        self.ix(
            accounts::Claim {
                user: *user,
                claim_signer: *signer,
                config: self.config,
                receipt: self.receipt_address(user),
                mint: self.mint,
                vault: self.vault,
                destination,
                token_program: spl_token::ID,
                system_program: anchor_lang::system_program::ID,
            },
            instruction::Claim { total },
        )
    }

    /// Creates the user's ATA if missing, then claims — the two instructions the
    /// app will send. The user is the fee payer.
    fn claim_with(
        &mut self,
        index: usize,
        co_signer: &Keypair,
        declared_signer: Pubkey,
        total: u64,
        destination: Option<Pubkey>,
    ) -> TransactionResult {
        let user = self.users[index].insecure_clone();
        let destination =
            destination.unwrap_or_else(|| get_associated_token_address(&user.pubkey(), &self.mint));

        let mut ixs = vec![];
        if self.svm.get_account(&destination).is_none() {
            ixs.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    &user.pubkey(),
                    &user.pubkey(),
                    &self.mint,
                    &spl_token::ID,
                ),
            );
        }
        ixs.push(self.claim_ix(&user.pubkey(), &declared_signer, total, destination));

        let co_signer = co_signer.insecure_clone();
        self.send(&ixs, &user, &[&user, &co_signer])
    }

    fn claim(&mut self, index: usize, total: u64) -> TransactionResult {
        let signer = self.claim_signer.insecure_clone();
        self.claim_with(index, &signer, signer.pubkey(), total, None)
    }

    /// Claims as a wallet that is not in `users` — how an attacker holding the
    /// signing key would use a keypair of its own.
    fn claim_as_outsider(&mut self, user: &Keypair, total: u64) -> TransactionResult {
        let signer = self.claim_signer.insecure_clone();
        let destination = get_associated_token_address(&user.pubkey(), &self.mint);

        let mut ixs = vec![];
        if self.svm.get_account(&destination).is_none() {
            ixs.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    &user.pubkey(),
                    &user.pubkey(),
                    &self.mint,
                    &spl_token::ID,
                ),
            );
        }
        ixs.push(self.claim_ix(&user.pubkey(), &signer.pubkey(), total, destination));

        let user = user.insecure_clone();
        self.send(&ixs, &user, &[&user, &signer])
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

    fn set_claim_signer(&mut self, signer: &Keypair, claim_signer: Pubkey) -> TransactionResult {
        let ix = self.ix(
            accounts::AdminOnly {
                admin: signer.pubkey(),
                config: self.config,
            },
            instruction::SetClaimSigner { claim_signer },
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

    fn claimed(&self, index: usize) -> u64 {
        let address = self.receipt_address(&self.users[index].pubkey());
        let account = self.svm.get_account(&address).unwrap();

        Receipt::try_deserialize(&mut account.data.as_slice())
            .unwrap()
            .claimed
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

fn assert_error(result: TransactionResult, expected: ClaimError) {
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

// -- happy paths -------------------------------------------------------------

#[test]
fn pays_a_first_claim() {
    let mut world = World::new(4, 100);

    world.claim(0, 10 * UNIT).unwrap();

    assert_eq!(world.balance_of(0), 10 * UNIT);
    assert_eq!(world.claimed(0), 10 * UNIT);
    assert_eq!(world.token_balance(&world.vault), 90 * UNIT);
}

/// The signed argument is a total, so the program pays the difference.
#[test]
fn incremental_claims_pay_only_the_difference() {
    let mut world = World::new(4, 100);

    world.claim(0, 10 * UNIT).unwrap();
    world.claim(0, 25 * UNIT).unwrap();

    assert_eq!(world.balance_of(0), 25 * UNIT);
    assert_eq!(world.token_balance(&world.vault), 75 * UNIT);
}

/// `init_if_needed` must not reinitialize an existing receipt.
#[test]
fn a_later_claim_does_not_reset_the_receipt() {
    let mut world = World::new(4, 100);

    world.claim(0, 10 * UNIT).unwrap();
    assert_eq!(world.claimed(0), 10 * UNIT);

    world.claim(0, 25 * UNIT).unwrap();
    assert_eq!(world.claimed(0), 25 * UNIT);

    world.claim(0, 26 * UNIT).unwrap();
    assert_eq!(world.claimed(0), 26 * UNIT);
    assert_eq!(world.balance_of(0), 26 * UNIT);
}

#[test]
fn receipts_are_per_wallet() {
    let mut world = World::new(3, 100);

    world.claim(0, 10 * UNIT).unwrap();
    world.claim(1, 30 * UNIT).unwrap();

    assert_eq!(world.claimed(0), 10 * UNIT);
    assert_eq!(world.claimed(1), 30 * UNIT);
    assert_eq!(world.balance_of(0), 10 * UNIT);
    assert_eq!(world.balance_of(1), 30 * UNIT);
}

#[test]
fn admin_can_rotate_the_claim_signer() {
    let mut world = World::new(4, 100);
    let replacement = Keypair::new();
    let retired = world.claim_signer.insecure_clone();

    let admin = world.admin.insecure_clone();
    world
        .set_claim_signer(&admin, replacement.pubkey())
        .unwrap();

    assert_constraint(
        world.claim_with(0, &retired, retired.pubkey(), 10 * UNIT, None),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );

    world.claim_signer = replacement;
    world.claim(0, 10 * UNIT).unwrap();
    assert_eq!(world.balance_of(0), 10 * UNIT);
}

#[test]
fn pause_and_unpause() {
    let mut world = World::new(4, 100);
    world.set_paused(true).unwrap();

    assert_error(world.claim(0, 10 * UNIT), ClaimError::Paused);

    world.set_paused(false).unwrap();
    world.claim(0, 10 * UNIT).unwrap();
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

// -- events ------------------------------------------------------------------

/// `owed` is the delta this transaction paid, `total` the receipt's new running
/// total. An incremental claim is where the two differ, which is the case a
/// listener reconciling against the entitlement ledger has to get right.
#[test]
fn a_claim_emits_both_the_delta_and_the_running_total() {
    let mut world = World::new(4, 100);
    let user = world.users[0].pubkey();

    let first = world.claim(0, 10 * UNIT);
    let event: Claimed = only_event(&first);
    assert_eq!(event.user, user);
    assert_eq!(event.owed, 10 * UNIT);
    assert_eq!(event.total, 10 * UNIT);

    let second = world.claim(0, 25 * UNIT);
    let event: Claimed = only_event(&second);
    assert_eq!(event.owed, 15 * UNIT);
    assert_eq!(event.total, 25 * UNIT);
}

/// The rotation this most needs to be alertable on: `claim_signer` is the key
/// that can pay out the vault.
#[test]
fn a_signer_rotation_emits_both_keys() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let before = world.config().claim_signer;
    let replacement = Keypair::new();

    let result = world.set_claim_signer(&admin, replacement.pubkey());

    let event: ClaimSignerUpdated = only_event(&result);
    assert_eq!(event.previous, before);
    assert_eq!(event.current, replacement.pubkey());
}

#[test]
fn the_pause_the_admin_and_a_withdrawal_are_all_observable() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();

    let result = world.set_paused(true);
    let event: PausedSet = only_event(&result);
    assert!(event.paused);

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

    // The successor is the admin from here, so the withdrawal is signed by it.
    let destination = get_associated_token_address(&successor.pubkey(), &world.mint);
    let create = spl_associated_token_account::instruction::create_associated_token_account(
        &successor.pubkey(),
        &successor.pubkey(),
        &world.mint,
        &spl_token::ID,
    );
    let withdraw = world.ix(
        accounts::Withdraw {
            admin: successor.pubkey(),
            config: world.config,
            mint: world.mint,
            vault: world.vault,
            destination,
            token_program: spl_token::ID,
        },
        instruction::Withdraw { amount: 60 * UNIT },
    );
    let result = world.send(&[create, withdraw], &successor, &[&successor]);

    let event: Withdrawn = only_event(&result);
    assert_eq!(event.amount, 60 * UNIT);
    assert_eq!(event.destination, destination);
}

/// A failed claim emits nothing — the log is a record of what happened, not of
/// what was attempted.
#[test]
fn a_refused_claim_emits_nothing() {
    let mut world = World::new(4, 100);
    world.claim(0, 10 * UNIT).unwrap();

    let result = world.claim(0, 10 * UNIT);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .meta
        .logs
        .iter()
        .all(|line| !line.starts_with("Program data: ")));
}

// -- claims that must fail ---------------------------------------------------

/// Resubmitting a transaction is not a double payment: the total is already
/// recorded, so there is nothing left owing.
#[test]
fn rejects_a_resubmitted_total() {
    let mut world = World::new(4, 100);
    world.claim(0, 10 * UNIT).unwrap();

    assert_error(world.claim(0, 10 * UNIT), ClaimError::NothingToClaim);
    assert_eq!(world.balance_of(0), 10 * UNIT);
}

#[test]
fn rejects_a_total_below_what_was_already_claimed() {
    let mut world = World::new(4, 100);
    world.claim(0, 25 * UNIT).unwrap();

    assert_error(world.claim(0, 10 * UNIT), ClaimError::NothingToClaim);
    assert_eq!(world.claimed(0), 25 * UNIT);
    assert_eq!(world.balance_of(0), 25 * UNIT);
}

#[test]
fn rejects_a_zero_total() {
    let mut world = World::new(4, 100);

    assert_error(world.claim(0, 0), ClaimError::NothingToClaim);
}

#[test]
fn rejects_a_claim_the_claim_signer_did_not_sign() {
    let mut world = World::new(4, 100);
    let user = world.users[0].insecure_clone();
    let signer = world.claim_signer.pubkey();
    let destination = get_associated_token_address(&user.pubkey(), &world.mint);

    let create = spl_associated_token_account::instruction::create_associated_token_account(
        &user.pubkey(),
        &user.pubkey(),
        &world.mint,
        &spl_token::ID,
    );
    let claim = world.claim_ix(&user.pubkey(), &signer, 10 * UNIT, destination);

    let failure = world
        .send_partially_signed(&[create, claim], &user, &[&user])
        .expect_err("an unsigned claim must not settle");

    assert_eq!(format!("{:?}", failure.err), "SignatureFailure");
    assert_eq!(world.balance_of(0), 0);
}

#[test]
fn rejects_a_signature_from_a_key_that_is_not_the_claim_signer() {
    let mut world = World::new(4, 100);
    let impostor = Keypair::new();

    assert_constraint(
        world.claim_with(0, &impostor, impostor.pubkey(), 10 * UNIT, None),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
}

#[test]
fn rejects_a_claim_the_vault_cannot_cover() {
    let mut world = World::new(4, 1);

    assert_error(world.claim(0, 10 * UNIT), ClaimError::InsufficientVault);
}

#[test]
fn rejects_a_destination_that_is_not_the_users_ata() {
    let mut world = World::new(4, 100);
    let elsewhere = get_associated_token_address(&world.users[2].pubkey(), &world.mint);

    let admin = world.admin.insecure_clone();
    let create = spl_associated_token_account::instruction::create_associated_token_account(
        &admin.pubkey(),
        &world.users[2].pubkey(),
        &world.mint,
        &spl_token::ID,
    );
    world.send(&[create], &admin, &[&admin]).unwrap();

    let signer = world.claim_signer.insecure_clone();
    assert_constraint(
        world.claim_with(0, &signer, signer.pubkey(), 10 * UNIT, Some(elsewhere)),
        anchor_lang::error::ErrorCode::ConstraintTokenOwner,
    );
    assert_eq!(world.token_balance(&elsewhere), 0);
}

// -- admin gating ------------------------------------------------------------

#[test]
fn rejects_set_claim_signer_from_a_non_admin() {
    let mut world = World::new(4, 100);
    let impostor = world.users[0].insecure_clone();

    assert_constraint(
        world.set_claim_signer(&impostor, impostor.pubkey()),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
    assert_eq!(world.config().claim_signer, world.claim_signer.pubkey());
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
    world.set_paused(true).unwrap();
    world.set_paused(false).unwrap();

    world.accept_admin(&successor).unwrap();

    assert_eq!(world.config().admin, successor.pubkey());
    assert_eq!(world.config().pending_admin, None);
    world
        .set_claim_signer(&successor, successor.pubkey())
        .unwrap();
    assert_constraint(
        world.set_claim_signer(&admin, admin.pubkey()),
        anchor_lang::error::ErrorCode::ConstraintHasOne,
    );
}

/// The finding this instruction exists for: a key nobody holds can be proposed,
/// and never becomes the admin because it cannot sign the second step. Here that
/// matters more than in the airdrop — `withdraw` is the only way tokens leave
/// this vault other than a co-signed claim.
#[test]
fn a_mistyped_successor_never_takes_the_key() {
    let mut world = World::new(4, 100);
    let admin = world.admin.insecure_clone();
    let unheld = Pubkey::new_unique();

    world.transfer_admin(&admin, Some(unheld)).unwrap();

    assert_eq!(world.config().admin, admin.pubkey());
    world.set_paused(true).unwrap();
    world.set_paused(false).unwrap();

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
    assert_error(world.accept_admin(&admin), ClaimError::NotPendingAdmin);

    world
        .transfer_admin(&admin, Some(successor.pubkey()))
        .unwrap();

    assert_error(world.accept_admin(&impostor), ClaimError::NotPendingAdmin);
    assert_error(world.accept_admin(&admin), ClaimError::NotPendingAdmin);
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

    assert_error(world.accept_admin(&successor), ClaimError::NotPendingAdmin);
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

    assert_error(world.accept_admin(&first), ClaimError::NotPendingAdmin);

    world.accept_admin(&second).unwrap();
    assert_eq!(world.config().admin, second.pubkey());
    assert_error(world.accept_admin(&first), ClaimError::NotPendingAdmin);
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

// -- the loss bound ----------------------------------------------------------

/// The standing record of what a leaked `claim_signer` costs: the vault, in
/// full, by two independent routes.
///
/// This is why no per-transaction or per-wallet ceiling was added. `total` is a
/// running total rather than a delta, so a single wallet can sit at any limit on
/// every call while its total climbs without bound; and a wallet the backend has
/// never seen starts from a zeroed receipt, so a per-wallet cap costs an
/// attacker one keypair. The bound is the vault balance and nothing else.
#[test]
fn a_leaked_signer_drains_the_vault() {
    let mut world = World::new(4, 100);

    // Route one: one wallet, total walked upward ten times.
    for i in 1..=5u64 {
        world.claim(0, i * 10 * UNIT).unwrap();
    }
    assert_eq!(world.claimed(0), 50 * UNIT);
    assert_eq!(world.balance_of(0), 50 * UNIT);

    // Route two: a keypair the entitlement ledger has never heard of.
    let outsider = Keypair::new();
    world.svm.airdrop(&outsider.pubkey(), SOL).unwrap();
    world.claim_as_outsider(&outsider, 50 * UNIT).unwrap();

    assert_eq!(world.token_balance(&world.vault), 0);
    assert_eq!(
        world.token_balance(&get_associated_token_address(
            &outsider.pubkey(),
            &world.mint
        )),
        50 * UNIT
    );
}

/// `paused` is the only lever against the above, and it is a Squads quorum
/// rather than a single signature — slower than the attack.
#[test]
fn pausing_is_the_only_thing_that_stops_a_leaked_signer() {
    let mut world = World::new(4, 100);
    world.claim(0, 10 * UNIT).unwrap();

    world.set_paused(true).unwrap();
    assert_error(world.claim(0, 20 * UNIT), ClaimError::Paused);

    let outsider = Keypair::new();
    world.svm.airdrop(&outsider.pubkey(), SOL).unwrap();
    assert_error(
        world.claim_as_outsider(&outsider, 90 * UNIT),
        ClaimError::Paused,
    );

    assert_eq!(world.token_balance(&world.vault), 90 * UNIT);
}

// -- documented consequences -------------------------------------------------

/// An admin withdrawal mid-flight leaves the receipt untouched: the claim
/// reverts whole rather than recording a payment that never moved.
#[test]
fn a_withdrawal_racing_a_claim_leaves_the_receipt_intact() {
    let mut world = World::new(4, 100);
    world.claim(0, 30 * UNIT).unwrap();

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
        instruction::Withdraw { amount: 70 * UNIT },
    );
    world.send(&[create, withdraw], &admin, &[&admin]).unwrap();

    // The entitlement still exists and is still authorized; there is nothing to
    // pay it with.
    assert_error(world.claim(0, 60 * UNIT), ClaimError::InsufficientVault);
    assert_eq!(world.claimed(0), 30 * UNIT);
    assert_eq!(world.balance_of(0), 30 * UNIT);
}

/// `claimed` only ever increases. No instruction in the program lowers or clears
/// it — true by absence today, and this is what keeps it true.
#[test]
fn nothing_in_the_program_can_lower_a_receipt() {
    let mut world = World::new(4, 100);
    world.claim(0, 40 * UNIT).unwrap();

    let admin = world.admin.insecure_clone();
    let successor = Keypair::new();
    world.svm.airdrop(&successor.pubkey(), SOL).unwrap();

    // Everything the admin surface can do, in sequence.
    world.set_claim_signer(&admin, world.claim_signer.pubkey()).unwrap();
    world.set_paused(true).unwrap();
    world.set_paused(false).unwrap();
    world
        .transfer_admin(&admin, Some(successor.pubkey()))
        .unwrap();
    world.accept_admin(&successor).unwrap();

    assert_eq!(world.claimed(0), 40 * UNIT);

    // And a lower total is still refused rather than rewriting the receipt.
    assert_error(world.claim(0, 39 * UNIT), ClaimError::NothingToClaim);
    assert_eq!(world.claimed(0), 40 * UNIT);
}
