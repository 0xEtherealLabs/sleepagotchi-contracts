//! Shared LiteSVM harness.
//!
//! Compiles in the program binary from `target/deploy/`, so run `pnpm stake:test`,
//! which builds first. A bare `cargo test` on a clean tree fails to compile.

// litesvm's `TransactionResult` carries failure metadata in its `Err` by design.
#![allow(clippy::result_large_err)]
// Each test binary uses a different slice of this.
#![allow(dead_code)]

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, program_pack::Pack, system_instruction},
    AccountDeserialize, InstructionData, ToAccountMetas,
};
use anchor_spl::{
    associated_token::{
        get_associated_token_address, spl_associated_token_account, ID as ATA_PROGRAM,
    },
    token::spl_token,
};
use litesvm::{types::TransactionResult, LiteSVM};
use sleepagotchi_stake::{
    accounts,
    error::StakeError,
    instruction,
    state::{Config, Position, Season, SeasonParams},
    CONFIG_SEED, MIN_SWEEP_DELAY_SECONDS, ONE_X_BPS, POSITION_SEED, REWARDS_SEED, SEASON_SEED,
};
use solana_clock::Clock;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

pub const PROGRAM: Pubkey = sleepagotchi_stake::ID;
pub const SOL: u64 = 1_000_000_000;
pub const DECIMALS: u8 = 9;
/// One whole $SLEEP in base units.
pub const UNIT: u64 = 1_000_000_000;
pub const MILLION: u64 = 1_000_000 * UNIT;

pub const NOW: i64 = 1_800_000_000;
pub const DAY: i64 = 86_400;
pub const START: i64 = NOW + 7 * DAY;
pub const DURATION: i64 = 90 * DAY;
pub const END: i64 = START + DURATION;

pub fn params() -> SeasonParams {
    SeasonParams {
        start_ts: START,
        end_ts: END,
        reward_pool: 2 * MILLION,
        max_total_staked: 100 * MILLION,
        max_per_wallet: MILLION,
        max_apr_bps: 1_500,
        max_multiplier_bps: 5 * ONE_X_BPS,
        sweep_delay_seconds: SWEEP_DELAY,
    }
}

/// The floor itself, so the sweep tests exercise the shortest window a season
/// can legally offer.
pub const SWEEP_DELAY: u32 = MIN_SWEEP_DELAY_SECONDS;

pub struct World {
    pub svm: LiteSVM,
    pub admin: Keypair,
    pub stake_signer: Keypair,
    pub mint: Pubkey,
    pub config: Pubkey,
    /// Admin's token account: the source seasons are funded from, and where a
    /// lowered pool is refunded to.
    pub treasury: Pubkey,
    pub users: Vec<Keypair>,
}

impl World {
    pub fn new() -> Self {
        Self::with_users(0)
    }

    /// `n` users, each holding 10M $SLEEP in their ATA.
    pub fn with_users(n: usize) -> Self {
        // History off so a resubmitted transaction reaches the program instead of
        // being deduplicated by the runtime.
        let mut svm = LiteSVM::new().with_transaction_history(0);
        svm.add_program(
            PROGRAM,
            include_bytes!("../../../../target/deploy/sleepagotchi_stake.so"),
        )
        .unwrap();

        let admin = Keypair::new();
        svm.airdrop(&admin.pubkey(), 1_000 * SOL).unwrap();

        // Never funded — it only ever attests a multiplier.
        let stake_signer = Keypair::new();

        let mint = Keypair::new();
        let (config, _) = Pubkey::find_program_address(&[CONFIG_SEED], &PROGRAM);
        let treasury = get_associated_token_address(&admin.pubkey(), &mint.pubkey());

        let users: Vec<Keypair> = (0..n).map(|_| Keypair::new()).collect();
        for user in &users {
            svm.airdrop(&user.pubkey(), 100 * SOL).unwrap();
        }

        let mut world = Self {
            svm,
            admin,
            stake_signer,
            mint: mint.pubkey(),
            config,
            treasury,
            users,
        };

        world.warp_to(NOW);
        world.create_mint(&mint);
        world.mint_to(&world.admin.pubkey(), 1_000 * MILLION);
        world.initialize().unwrap();
        for index in 0..n {
            let user = world.users[index].pubkey();
            world.mint_to(&user, 10 * MILLION);
        }
        world
    }

    pub fn warp_to(&mut self, unix_timestamp: i64) {
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

    /// Creates `owner`'s ATA if missing, then mints into it.
    pub fn mint_to(&mut self, owner: &Pubkey, amount: u64) {
        let admin = self.admin.insecure_clone();
        let ata = get_associated_token_address(owner, &self.mint);

        let mut ixs = vec![];
        if self.svm.get_account(&ata).is_none() {
            ixs.push(
                spl_associated_token_account::instruction::create_associated_token_account(
                    &admin.pubkey(),
                    owner,
                    &self.mint,
                    &spl_token::ID,
                ),
            );
        }
        ixs.push(
            spl_token::instruction::mint_to(
                &spl_token::ID,
                &self.mint,
                &ata,
                &admin.pubkey(),
                &[],
                amount,
            )
            .unwrap(),
        );
        self.send(&ixs, &admin, &[&admin]).unwrap();
    }

    // -- addresses ---------------------------------------------------------

    pub fn season_address(&self, id: u64) -> Pubkey {
        Pubkey::find_program_address(&[SEASON_SEED, &id.to_le_bytes()], &PROGRAM).0
    }

    pub fn rewards_authority(&self, id: u64) -> Pubkey {
        Pubkey::find_program_address(&[REWARDS_SEED, &id.to_le_bytes()], &PROGRAM).0
    }

    pub fn stake_vault(&self, id: u64) -> Pubkey {
        get_associated_token_address(&self.season_address(id), &self.mint)
    }

    pub fn reward_vault(&self, id: u64) -> Pubkey {
        get_associated_token_address(&self.rewards_authority(id), &self.mint)
    }

    pub fn position_address(&self, id: u64, user: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[POSITION_SEED, &id.to_le_bytes(), user.as_ref()],
            &PROGRAM,
        )
        .0
    }

    pub fn token_account(&self, owner: &Pubkey) -> Pubkey {
        get_associated_token_address(owner, &self.mint)
    }

    // -- instructions ------------------------------------------------------

    pub fn initialize(&mut self) -> TransactionResult {
        let admin = self.admin.insecure_clone();
        let ix = self.ix(
            accounts::Initialize {
                admin: admin.pubkey(),
                config: self.config,
                mint: self.mint,
                system_program: anchor_lang::system_program::ID,
            },
            instruction::Initialize {
                stake_signer: self.stake_signer.pubkey(),
            },
        );
        self.send(&[ix], &admin, &[&admin])
    }

    pub fn open_season_as(&mut self, admin: &Keypair, params: SeasonParams) -> TransactionResult {
        let id = self.config().next_season;
        let ix = self.ix(
            accounts::OpenSeason {
                admin: admin.pubkey(),
                config: self.config,
                season: self.season_address(id),
                rewards_authority: self.rewards_authority(id),
                mint: self.mint,
                stake_vault: self.stake_vault(id),
                reward_vault: self.reward_vault(id),
                funding: self.treasury,
                token_program: spl_token::ID,
                associated_token_program: ATA_PROGRAM,
                system_program: anchor_lang::system_program::ID,
            },
            instruction::OpenSeason { params },
        );
        let admin = admin.insecure_clone();
        self.send(&[ix], &admin, &[&admin])
    }

    pub fn open_season(&mut self, params: SeasonParams) -> TransactionResult {
        let admin = self.admin.insecure_clone();
        self.open_season_as(&admin, params)
    }

    pub fn update_season_as(
        &mut self,
        admin: &Keypair,
        id: u64,
        params: SeasonParams,
    ) -> TransactionResult {
        let ix = self.ix(
            accounts::UpdateSeason {
                admin: admin.pubkey(),
                config: self.config,
                season: self.season_address(id),
                rewards_authority: self.rewards_authority(id),
                mint: self.mint,
                reward_vault: self.reward_vault(id),
                funding: self.treasury,
                token_program: spl_token::ID,
            },
            instruction::UpdateSeason { params },
        );
        let admin = admin.insecure_clone();
        self.send(&[ix], &admin, &[&admin])
    }

    pub fn update_season(&mut self, id: u64, params: SeasonParams) -> TransactionResult {
        let admin = self.admin.insecure_clone();
        self.update_season_as(&admin, id, params)
    }

    pub fn stake_ix(
        &self,
        user: &Pubkey,
        declared_signer: &Pubkey,
        id: u64,
        amount: u64,
        multiplier_bps: u32,
    ) -> Instruction {
        self.ix(
            accounts::Stake {
                user: *user,
                stake_signer: *declared_signer,
                config: self.config,
                season: self.season_address(id),
                position: self.position_address(id, user),
                mint: self.mint,
                stake_vault: self.stake_vault(id),
                source: self.token_account(user),
                token_program: spl_token::ID,
                system_program: anchor_lang::system_program::ID,
            },
            instruction::Stake {
                amount,
                multiplier_bps,
            },
        )
    }

    pub fn unstake_ix(&self, user: &Pubkey, id: u64, amount: u64) -> Instruction {
        self.ix(
            accounts::Unstake {
                user: *user,
                config: self.config,
                season: self.season_address(id),
                position: self.position_address(id, user),
                mint: self.mint,
                stake_vault: self.stake_vault(id),
                destination: self.token_account(user),
                token_program: spl_token::ID,
            },
            instruction::Unstake { amount },
        )
    }

    /// Signed by the user and by whichever key is passed as the co-signer,
    /// which is not necessarily the one declared in the accounts.
    pub fn stake_signed_by(
        &mut self,
        index: usize,
        co_signer: &Keypair,
        declared_signer: Pubkey,
        id: u64,
        amount: u64,
        multiplier_bps: u32,
    ) -> TransactionResult {
        let user = self.users[index].insecure_clone();
        let ix = self.stake_ix(&user.pubkey(), &declared_signer, id, amount, multiplier_bps);
        let co_signer = co_signer.insecure_clone();
        self.send(&[ix], &user, &[&user, &co_signer])
    }

    pub fn stake(
        &mut self,
        index: usize,
        id: u64,
        amount: u64,
        multiplier_bps: u32,
    ) -> TransactionResult {
        let signer = self.stake_signer.insecure_clone();
        self.stake_signed_by(index, &signer, signer.pubkey(), id, amount, multiplier_bps)
    }

    pub fn unstake(&mut self, index: usize, id: u64, amount: u64) -> TransactionResult {
        let user = self.users[index].insecure_clone();
        let ix = self.unstake_ix(&user.pubkey(), id, amount);
        self.send(&[ix], &user, &[&user])
    }

    pub fn claim_ix(&self, user: &Pubkey, id: u64) -> Instruction {
        self.ix(
            accounts::Claim {
                user: *user,
                config: self.config,
                season: self.season_address(id),
                position: self.position_address(id, user),
                rewards_authority: self.rewards_authority(id),
                mint: self.mint,
                reward_vault: self.reward_vault(id),
                destination: self.token_account(user),
                token_program: spl_token::ID,
            },
            instruction::Claim {},
        )
    }

    pub fn claim(&mut self, index: usize, id: u64) -> TransactionResult {
        let user = self.users[index].insecure_clone();
        let ix = self.claim_ix(&user.pubkey(), id);
        self.send(&[ix], &user, &[&user])
    }

    pub fn sweep_ix(&self, admin: &Pubkey, id: u64, destination: Pubkey) -> Instruction {
        self.ix(
            accounts::SweepUnclaimed {
                admin: *admin,
                config: self.config,
                season: self.season_address(id),
                rewards_authority: self.rewards_authority(id),
                mint: self.mint,
                reward_vault: self.reward_vault(id),
                destination,
                token_program: spl_token::ID,
            },
            instruction::SweepUnclaimed {},
        )
    }

    pub fn sweep_as(&mut self, admin: &Keypair, id: u64, destination: Pubkey) -> TransactionResult {
        let ix = self.sweep_ix(&admin.pubkey(), id, destination);
        let admin = admin.insecure_clone();
        self.send(&[ix], &admin, &[&admin])
    }

    pub fn sweep(&mut self, id: u64) -> TransactionResult {
        let admin = self.admin.insecure_clone();
        let treasury = self.treasury;
        self.sweep_as(&admin, id, treasury)
    }

    pub fn admin_ix(&mut self, admin: &Keypair, data: impl InstructionData) -> TransactionResult {
        let ix = self.ix(
            accounts::AdminOnly {
                admin: admin.pubkey(),
                config: self.config,
            },
            data,
        );
        let admin = admin.insecure_clone();
        self.send(&[ix], &admin, &[&admin])
    }

    pub fn transfer_admin(
        &mut self,
        signer: &Keypair,
        new_admin: Option<Pubkey>,
    ) -> TransactionResult {
        self.admin_ix(signer, instruction::TransferAdmin { new_admin })
    }

    pub fn accept_admin(&mut self, signer: &Keypair) -> TransactionResult {
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

    pub fn set_paused(&mut self, paused: bool) {
        let admin = self.admin.insecure_clone();
        self.admin_ix(&admin, instruction::SetPaused { paused })
            .unwrap();
    }

    // -- plumbing ----------------------------------------------------------

    pub fn ix(&self, accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
        Instruction {
            program_id: PROGRAM,
            accounts: accounts.to_account_metas(None),
            data: data.data(),
        }
    }

    pub fn send(
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
    pub fn send_partially_signed(
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

    // -- reads -------------------------------------------------------------

    pub fn config(&self) -> Config {
        let account = self.svm.get_account(&self.config).expect("no config");
        Config::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn season(&self, id: u64) -> Season {
        let account = self
            .svm
            .get_account(&self.season_address(id))
            .expect("no season");
        Season::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn position(&self, id: u64, index: usize) -> Position {
        let account = self
            .svm
            .get_account(&self.position_address(id, &self.users[index].pubkey()))
            .expect("no position");
        Position::try_deserialize(&mut account.data.as_slice()).unwrap()
    }

    pub fn balance(&self, token_account: &Pubkey) -> u64 {
        self.svm
            .get_account(token_account)
            .map(|a| spl_token::state::Account::unpack(&a.data).unwrap().amount)
            .unwrap_or_default()
    }

    pub fn user_balance(&self, index: usize) -> u64 {
        self.balance(&self.token_account(&self.users[index].pubkey()))
    }
}

/// Deterministic splitmix64. The lib crate's copy is `cfg(test)`, which an
/// integration test does not link.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}

/// The ceiling a position can be paid: `max_apr_bps` applied to raw
/// stake-seconds, which is the multiplier-free integral.
pub fn apr_cap(raw_stake_seconds: u128, max_apr_bps: u16) -> u128 {
    raw_stake_seconds * max_apr_bps as u128 / (10_000u128 * 31_536_000)
}

/// Decodes every Anchor event of type `E` out of a successful transaction's logs.
///
/// `emit!` base64-encodes `discriminator || borsh(event)` behind a `Program
/// data:` line, so this is the same path an off-chain listener takes — which is
/// what makes it a test of the monitoring surface rather than of the macro.
pub fn events_of<E>(result: &TransactionResult) -> Vec<E>
where
    E: anchor_lang::Event + anchor_lang::Discriminator + anchor_lang::AnchorDeserialize,
{
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

pub fn only_event<E>(result: &TransactionResult) -> E
where
    E: anchor_lang::Event + anchor_lang::Discriminator + anchor_lang::AnchorDeserialize,
{
    let mut found = events_of::<E>(result);

    assert_eq!(found.len(), 1, "expected exactly one event");
    found.remove(0)
}

pub fn assert_code(result: TransactionResult, code: u32) {
    let failure = result.expect_err("expected the transaction to fail");
    assert!(
        format!("{:?}", failure.err).contains(&format!("Custom({code})")),
        "expected Custom({code}), got {:?}\nlogs: {:#?}",
        failure.err,
        failure.meta.logs,
    );
}

pub fn assert_error(result: TransactionResult, expected: StakeError) {
    // `#[error_code]` numbers variants from zero and adds the offset on conversion.
    assert_code(
        result,
        expected as u32 + anchor_lang::error::ERROR_CODE_OFFSET,
    );
}

/// Anchor's own constraint codes already carry their absolute number.
pub fn assert_constraint(result: TransactionResult, expected: anchor_lang::error::ErrorCode) {
    assert_code(result, expected as u32);
}
