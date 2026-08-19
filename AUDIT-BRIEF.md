# Sleepagotchi Solana programs — audit brief

Prepared for external security review.

## 1. Scope

Three Anchor programs. The revision under review is recorded in the engagement
scope rather than here.

| Program | Path | Custodies |
| --- | --- | --- |
| `sleepagotchi_airdrop` | `programs/sleepagotchi_airdrop` | A fixed allocation list, funded to the tree total |
| `sleepagotchi_claim` | `programs/sleepagotchi_claim` | A working float for continuously accruing entitlement |
| `sleepagotchi_stake` | `programs/sleepagotchi_stake` | User principal and one reward pool per season |

Program ids:

```
sleepagotchi_airdrop   DM6GCMdgn9daiiUCCaiCjrQHWwe3RiNJK3EPcqa2sdBg
sleepagotchi_claim     8igHauFjDNKpTkyATw9cigy7XWxWMbvHotA4S9i4qe1f
sleepagotchi_stake     AbXC8pN6zbyoi3qWxLcMQC3yXhNUHmXpuDJ1bVDadjKG
```

Toolchain: Rust 1.89.0, Anchor 1.1.2, `anchor-spl` 1.1.2.

**Out of scope.** `token/` contains deployment scripts for a plain SPL mint with
no on-chain program; its properties are treated as an assumption in §4. The
TypeScript application that constructs transactions is out of scope, with two
exceptions noted in §6 where an on-chain guarantee depends on an off-chain
invariant.

Per-program detail — account layouts, PDA seeds, instruction signatures — is in
`claim-programs.md` and `staking-program.md`.

## 2. Architecture

All three programs are pull-based. The claimant submits and pays for the
transaction; a PDA that is the authority on the vault signs the transfer out. No
program holds a hot wallet, and no signing key is funded.

Vaults are associated token accounts of an authority PDA. No vault address is
persisted in program state, so vault identity is established by constraint
rather than by a stored pubkey. `sleepagotchi_stake` derives two authority PDAs
per season — `[b"season", id]` for principal and `[b"rewards", id]` for the
reward pool — so the two balances sit under distinct owners.

Authorization differs per program:

- `sleepagotchi_airdrop` verifies a Merkle proof against a root in `Config`. No
  signing key exists.
- `sleepagotchi_claim` requires a co-signature from `claim_signer` on the
  transaction. The signature covers the full message, so it binds the recipient
  and the amount without a separately encoded message.
- `sleepagotchi_stake` requires a co-signature from `stake_signer` on `stake`
  only, attesting a multiplier that stands in for an ownership check the program
  cannot perform. `unstake` and `claim` require no co-signature.

## 3. Privileged roles

| Role | Capabilities | Explicitly cannot |
| --- | --- | --- |
| `admin` (all three; Squads multisig) | Modify any `Config` field; withdraw either claim vault in full at any time with no timelock; open and modify unstarted seasons; sweep a closed season's remainder; pause | Move staked principal; prevent `unstake` or `claim`; modify a season after `start_ts` |
| `claim_signer` (`sleepagotchi_claim`) | Authorize a transfer of any amount up to the vault balance | Mint, move a vault directly, modify `Config`, or affect another wallet's receipt |
| `stake_signer` (`sleepagotchi_stake`) | Attest `multiplier_bps` bounded by the season's `max_multiplier_bps` | Any other state transition |

Admin handover is two-step in all three programs: `transfer_admin` records a
proposal, and the proposed key must sign `accept_admin`.

The strongest claim made by the system, and the one whose falsification would be
most severe:

**No privileged role can prevent or redirect a withdrawal from
`sleepagotchi_stake`.** `unstake` and `claim` carry no co-signature requirement,
no `paused` check and no window check.

## 4. Security assumptions

**Mint.** $SLEEP is a legacy SPL Token mint, fixed supply, mint and freeze
authorities revoked, with no transfer fee, transfer hook or confidential
transfer extension. Vault accounting assumes a transfer moves exactly the amount
requested.

This is enforced structurally: every account struct uses `Program<Token>` and
`Account<TokenAccount>`, never `token_interface`, so a Token-2022 mint cannot be
substituted. The `token_2022` Cargo feature is present in each manifest solely
because `anchor-spl` 1.1.2's `idl_build.rs` references the module
unconditionally; it is unreachable from any account struct.

**Clock.** Season and window boundaries derive from `Clock::unix_timestamp`,
which validators may skew by minutes. All boundaries are measured in days or
longer.

**Arithmetic.** `overflow-checks = true` in every release profile. Accrual paths
use checked arithmetic mapping to `MathOverflow`.

## 5. Invariants

**Accrual** (`programs/sleepagotchi_stake/src/accrual.rs`). Once every
position has been settled to `end_ts`:

```
Σ Position.weighted_stake_seconds == Season.weighted_stake_seconds
```

Both integrals advance over the same partition of time. `Season.total_weighted`
changes only at the instants a position is settled, and both accumulators are
advanced from their own `last_update_ts` before any mutation. Settlement is a
single helper invoked at the head of `stake`, `unstake` and `claim`; it is not
exposed as separable halves, since advancing a position without its season is
the failure mode the invariant exists to detect.

**Solvency.** `Σ reward_i ≤ Σ pro_rata_i` because each reward is a `min` against
the APR cap, and `Σ floor(pool × WSS_i / W) ≤ pool` because `Σ WSS_i = W` and
flooring is monotonic downward. Funding a reward vault with exactly
`reward_pool` is therefore sufficient.

**Precision** (`programs/sleepagotchi_stake/src/math.rs`). All values
remain within `u128` except the pro-rata numerator: `reward_pool ×
weighted_stake_seconds` reaches approximately 3.2e48 against a `u128` maximum of
3.4e38. That path uses `mul_div_floor`, a 256-bit intermediate implemented as
schoolbook multiplication on `u64` limbs followed by binary long division. The
wide path is the common case rather than an edge case: a 2,000,000 $SLEEP pool
against a wallet holding 1,000,000 at 2× for ninety days yields a 138-bit
product.

**Replay.** `sleepagotchi_airdrop` uses `init` on a receipt PDA seeded by wallet,
so a second claim fails during account resolution. `sleepagotchi_claim` takes a
running total rather than a delta, so a resubmitted transaction fails
`NothingToClaim`. `sleepagotchi_stake` gates on a `claimed` flag per position.

## 6. Accepted design decisions

The following are deliberate and known. They are documented here so that review
effort is not spent rediscovering them; assessment of whether the reasoning holds
remains in scope.

**A compromised `claim_signer` can drain the `sleepagotchi_claim` vault.** No
on-chain ceiling constrains it. A per-transaction limit was implemented and
subsequently reverted after testing demonstrated it does not bind: `claim` takes
a running total, so a single wallet can raise its total incrementally and remain
at the limit on every call; freshly generated keypairs each receive a zeroed
receipt; and the shared account set permits many claim instructions per
transaction. A per-wallet lifetime cap fails by the same mechanism. A cumulative
rolling-window limiter would bind, at the cost of serializing every claim behind
a single `Config` write. The loss bound is the vault balance.

**Admin withdrawal is unrestricted in both claim programs**, including while a
claim window is open. Subsequent claims fail `InsufficientVault`.

**A sweep may strand a claimant.** After `end_ts + sweep_delay_seconds`, the
admin may transfer a season's entire remaining reward vault. The delay carries a floor
(`MIN_SWEEP_DELAY_SECONDS`) because claims are impossible before `end_ts`; absent a floor there would always exist an interval in which the whole
pool is sweepable and nothing has been claimed. Past the delay the exposure is
bounded, not eliminated.

**Capped surplus in `sleepagotchi_stake` is not redistributed.** Redistribution
requires the set of capped positions, which is O(n) and not computable on chain.
A season may therefore underspend its pool.

**The staking multiplier is sticky within a season.** It is re-attested only on
`stake`. Currently unreachable in production: seasons are opened with
`max_multiplier_bps == ONE_X_BPS`, making 1× the only value the program accepts.

**Airdrop eligibility is fixed at snapshot.** `set_root` can admit wallets that
have not yet claimed; a wallet holding a receipt cannot be topped up.

**Tokens transferred directly to a stake vault are unrecoverable.** No admin
instruction reaches that vault, which is what makes the §3 withdrawal guarantee
structural.

**Receipts and positions are never closed** and their rent is not reclaimable.
Closing airdrop receipts would be unsound because `set_window` can reopen a
closed window.

**Overlapping seasons are prevented off chain.** `open_season` places no
constraint relating a new season's `start_ts` to any prior `end_ts`. Concurrent
seasons would allow one wallet to earn two APR caps simultaneously.

**One leaf per wallet is an off-chain invariant.** The airdrop receipt is seeded
by wallet alone and does not record which allocation it satisfied, so a wallet
appearing twice in the tree has one allocation permanently unclaimable.
Uniqueness is enforced during tree construction.

**Error variants are append-only.** `#[error_code]` numbers variants by
position; inserting one renumbers every variant below it.

## 7. Suggested focus

1. `sleepagotchi_stake` accrual and settlement — the invariant in §5, the
   settlement ordering around mutations, and `mul_div_floor`. This program holds
   the largest balance.
2. `sleepagotchi_claim` authorization — specifically whether the assessment in §6
   is correct that no per-transaction or per-wallet bound is enforceable.
3. `sleepagotchi_airdrop` Merkle verification and the on-chain/off-chain boundary
   around tree construction.

## 8. Build and verification

```bash
anchor build && cargo test
```

One workspace, three programs. The suite is weighted towards negative cases;
each guard has a test that breaks it. Two are of particular relevance to review:

- The stake accrual invariant is asserted against randomised traffic — many
  positions over irregular time steps, including zero-length ones — checking both
  `Σ Position.WSS == Season.WSS` and `Σ reward_i ≤ reward_pool`.
- The Merkle tree and the accrual arithmetic are additionally implemented in
  TypeScript for the application's projections. Committed fixtures
  (`fixtures/tree.json`, `fixtures/accrual.json`) are asserted from
  both implementations.

`scripts/stake/check.ts`, run via `pnpm check`, validates the hand-rolled Borsh
encoders and readers in `scripts/` against the committed IDL offline.

`Cargo.lock` is committed. Upgrade authorities are scheduled
to move to Squads and reproducible builds via `solana-verify` are scheduled
before mainnet deployment; neither is complete at the time of writing, so
deployed bytecode is not currently pinned to this tree.
