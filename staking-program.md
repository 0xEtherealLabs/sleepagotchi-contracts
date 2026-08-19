# Staking program

`sleepagotchi_stake` locks $SLEEP for the length of a season and pays $SLEEP out
of a fixed reward pool, split pro-rata by stake over time and capped per wallet
at an APR.

Its own Cargo workspace, its own Anchor toolchain, and vaults separate from the
claim programs' — this is the one holding user principal, so a compromise on any
other surface must not reach it.

| Program | Id |
| --- | --- |
| `sleepagotchi_stake` | `AbXC8pN6zbyoi3qWxLcMQC3yXhNUHmXpuDJ1bVDadjKG` |

Deployed addresses per cluster are in `deployments/`.

## Trust assumptions

**No privileged party can block or divert a withdrawal.** `unstake` and `claim`
require no co-signature, are not gated by `paused`, and have no window check, so
principal and earned rewards remain recoverable if the backend is offline, the
admin key is lost, the program is paused, or a season turns out to be
misconfigured. This is the most important property in the program and nothing
may be added later that weakens it.

**`admin`** — a Squads multisig. Can open a season, rewrite any season parameter
before it starts, rotate the stake signer, pause new stakes, hand the role on,
and sweep a closed season's unclaimed remainder. It cannot touch principal: the
stake vault's authority is the season PDA, and no admin instruction signs with
it. `sweep_unclaimed` reaches the reward vault only, so a sweep cannot take
principal whatever account it is pointed at.

Handover is two-step. `transfer_admin` proposes and the proposed key must sign
`accept_admin`, so a mistyped address never becomes the admin. Milder here than
in the claim programs — a key nobody holds strands no user funds, only
`open_season` and `sweep_unclaimed`.

**`stake_signer`** — co-signs every `stake` and attests `multiplier_bps`. It is
never funded. It attests one number and nothing else: it cannot mint, move a
vault, or affect another wallet's position, and the value it attests is bounded
by the season's `max_multiplier_bps`. Seasons are currently opened with that
ceiling equal to `ONE_X_BPS`, which makes the multiplier's absence structural
rather than a promise — with the ceiling equal to the floor, the only value the
program accepts is 1x, so a compromised signer cannot weight a position either.

**A live season has no admin lever.** Once `start_ts` passes, every parameter is
frozen. `paused` blocks new stakes and deliberately does not halt accrual, since
halting it would cut rewards people had already earned. A season that turns out
to be misconfigured runs to its end date — which costs stakers nothing, because
`unstake` is permissionless throughout.

**The sweep can still strand a late claimant.** After `end_ts +
sweep_delay_seconds` the admin may take the entire remaining reward vault,
including rewards earned by wallets that have not claimed. The delay bounds that
exposure; it does not remove it.

## The reward model

For a position `i` in a season running `[start, end]`:

```
weight_i(t) = amount_i(t) × multiplier_bps_i(t)
WSS_i       = ∫ weight_i dt        weighted stake-seconds
RSS_i       = ∫ amount_i dt        raw stake-seconds, multiplier excluded
W           = ∫ Σ weight_i dt      the same integral, tracked globally
```

At any time after `end`:

```
                    pool × WSS_i          max_apr_bps × RSS_i
reward_i = min(  ───────────────── ,  ───────────────────────────  )
                          W             10_000 × SECONDS_PER_YEAR
```

The left term is the pro-rata share, the right is the APR cap, and the `min` of
the two is floored before it is paid.

`SECONDS_PER_YEAR` is 31 536 000, a constant in the program rather than a
derived value.

Three consequences worth stating:

- **The multiplier moves share, never the ceiling.** It scales weight in the
  pro-rata split, while the cap is applied to `RSS_i`, which excludes it. So
  weighting decides how the pool is divided and never how much one wallet can
  take out of it.
- **The cap is measured on stake-seconds, not on the closing balance.** A wallet
  that holds a large position all season and unstakes to dust before `end_ts`
  still has its cap computed on what it actually held. Cycling cannot push
  `RSS_i` above `max_per_wallet × duration`.
- **Capped surplus is not redistributed.** Where a wallet is capped, the
  difference between its share and its cap stays in the vault. Redistribution
  would require knowing which wallets were capped, which is O(n) and cannot run
  on chain. A season can therefore underspend its pool; the remainder is swept.

`max_apr_bps × max_total_staked × duration / year` is the most a season can pay
however large the pool is. Past that point, extra pool cannot be claimed by
anyone.

### Properties

- **Same-slot stake and unstake earns nothing.** Weighting is an integral and a
  zero-length interval integrates to zero, so no cooldown is needed.
- **There is no end-of-season snapshot to front-run.** Staking a large amount one
  second before `end_ts` buys one second of stake-seconds.
- **The multiplier is not retroactive.** Settlement runs before the multiplier is
  overwritten, so a wallet keeps the weighting it held while it earned it.

## State

Three accounts. `Config` is a singleton; `Season` and `Position` are per season,
so a wallet's history in one season is independent of every other.

| Account | Seeds | Fields |
| --- | --- | --- |
| `Config` | `[b"config"]` | `admin`, `pending_admin`, `stake_signer`, `mint`, `paused`, `next_season`, `bump` |
| `Season` | `[b"season", id_le]` | `id`, `params`, `total_staked`, `total_weighted`, `weighted_stake_seconds`, `last_update_ts`, `rewards_paid`, `swept`, `bump`, `rewards_bump` |
| `Position` | `[b"position", id_le, user]` | `amount`, `multiplier_bps`, `weighted_stake_seconds`, `raw_stake_seconds`, `last_update_ts`, `claimed`, `bump` |

`SeasonParams` holds `start_ts`, `end_ts`, `reward_pool`, `max_total_staked`,
`max_per_wallet`, `max_apr_bps`, `max_multiplier_bps` and
`sweep_delay_seconds`. It is one struct rather than eight loose fields because
`open_season` and `update_season` both take it whole — which makes the freeze
rule a single assignment being refused, rather than a per-field check that a
parameter added later could escape.

`sweep_delay_seconds` has a floor of `MIN_SWEEP_DELAY_SECONDS`, seven days.
Claims are impossible before `end_ts`, so without a floor there would always be
an interval in which the whole pool is sweepable and nothing has been claimed.

### Two vaults per season

| Vault | Authority | Holds |
| --- | --- | --- |
| Stake vault | `[b"season", id_le]` | User principal, only |
| Reward vault | `[b"rewards", id_le]` | Exactly `reward_pool` |

Both are associated token accounts of their authority PDA, so neither address is
stored and neither can be passed wrong. Two authority PDAs rather than one
because an ATA is unique per owner and mint, and the same mint on both sides is
precisely why they must not share an owner: an accounting error in the reward
path then cannot reach principal, structurally, regardless of the arithmetic.

### The invariant

```
Σ Position.weighted_stake_seconds == Season.weighted_stake_seconds
```

once every position has been settled to `end_ts`. Both integrals advance over
the same partition of time: `Season.total_weighted` changes only at the instants
a position is settled, and both accumulators are advanced from their own
`last_update_ts` before any mutation.

Settlement is one helper called at the top of `stake`, `unstake` and `claim`,
before any mutation. It is deliberately not two public halves — advancing a
position without its season, or the reverse, is exactly the bug this invariant
exists to catch, so the API does not offer the mistake.

The reward vault is always solvent as a consequence: `Σ reward_i ≤ Σ pro_rata_i`
because each term is a `min`, and `Σ floor(pool × WSS_i / W) ≤ pool` because
`Σ WSS_i = W` and flooring only loses value.

## Instructions

| Instruction | Signers | Notes |
| --- | --- | --- |
| `initialize(stake_signer)` | admin | Singleton `Config` |
| `open_season(params)` | admin | Creates the season and both vaults, and moves `reward_pool` into the reward vault in the same instruction |
| `update_season(params)` | admin | Rewrites every parameter. Only before `start_ts` |
| `stake(amount, multiplier_bps)` | user **+ stake_signer** | Settles, then applies |
| `unstake(amount)` | user | No co-signature, no pause check, no window check |
| `claim()` | user | After `end_ts`, once per position. Returns no principal |
| `sweep_unclaimed()` | admin | Reward vault only, after `end_ts + sweep_delay_seconds` |
| `set_stake_signer(key)` | admin | |
| `transfer_admin(new_admin)` | admin | Proposes, or cancels with `None` |
| `accept_admin()` | pending admin | Completes the handover |
| `set_paused(paused)` | admin | Blocks new stakes only |

Funding is not a separate step from `open_season`: `reward_vault.amount ==
params.reward_pool` holds from the moment the season exists, so there is no
window in which a season is open and under-funded. An `update_season` that
changes the pool moves the difference in or out in the same instruction.

Every instruction that moves tokens or changes a season's economics emits an
event. `RewardClaimed` carries both of the position's integrals and the season's
denominator, so a payout can be re-derived from the log; `SweptUnclaimed` is the
one an operator should always alert on.

### Rolling into the next season

Carrying a position forward needs no dedicated instruction. `unstake` from the
old season and `stake` into the new one, composed in one transaction, is atomic,
is a single wallet approval, and moves principal vault to vault. The multiplier
is re-attested because the second instruction is an ordinary `stake`, and
nothing accrued carries across — each season's pool is split by that season's
stake-seconds.

## Arithmetic

`overflow-checks = true` in the release profile: every accrual path multiplies a
balance by elapsed seconds, and a silent wrap there is a wrong payout rather than
a crash.

Everything stays inside `u128` except the pro-rata numerator. `reward_pool ×
weighted_stake_seconds` exceeds `u128` at ordinary season magnitudes — a 2M
$SLEEP pool against a wallet holding 1M at 2x for ninety days is already a
138-bit product — so that one path uses a 256-bit intermediate, in `math.rs`:

```rust
/// floor(a × b / denom), exact. `None` on denom == 0 or a quotient above u128.
pub fn mul_div_floor(a: u128, b: u128, denom: u128) -> Option<u128>
```

Schoolbook 128→256 on `u64` limbs for the product, then binary long division for
the quotient. No dependency: it is one function, and a dependency here would be a
dependency inside the audit boundary.

Flooring is load-bearing rather than incidental — every reward rounds down, so
the sum of all of them cannot exceed the pool that funded them.

## Build and test

See [README.md](README.md) — one `anchor build` covers all three programs.
