# Claim programs

Two Anchor programs that pay $SLEEP out of a vault the program itself controls.
Both are pull-based: the user submits and pays for the transaction, and a PDA
signs the transfer out. Neither is the push airdrop, which involves no program at
all and lives in [token/README.md](token/README.md#push-airdrop).

| Program | Pays for | Authorization |
| --- | --- | --- |
| `sleepagotchi_airdrop` | A fixed allocation list, published as a Merkle root | A Merkle proof. There is no signing key. |
| `sleepagotchi_claim` | Amounts only known at request time | A backend co-signature on the transaction |

They are one shape with one field swapped. The airdrop's allocations are fixed
at snapshot time, so
they can be committed on chain up front; the claim program's entitlement grows
continuously, so a published root would need constant republishing.

**The vaults are separate, and that is the point.** A shared vault would let a
compromised claim signer drain airdrop funds, which would hand the airdrop back
the risk the Merkle design removes.

| Program | Id |
| --- | --- |
| `sleepagotchi_airdrop` | `DM6GCMdgn9daiiUCCaiCjrQHWwe3RiNJK3EPcqa2sdBg` |
| `sleepagotchi_claim` | `8igHauFjDNKpTkyATw9cigy7XWxWMbvHotA4S9i4qe1f` |

Deployed addresses per cluster are in `deployments/`.

## Trust assumptions

What each key can do, and what none of them can.

**`admin`** — a Squads multisig on both programs. Can change every field in
`Config`, including handing the role on, and can `withdraw` the entire vault at
any time. There is no timelock and no restriction to a particular phase: an
admin withdrawal during an open claim window is permitted, and every subsequent
claim then fails `InsufficientVault`. Admin compromise means loss of whatever
the vault holds, on both programs.

Handover is two-step. `transfer_admin` proposes, and the proposed key must sign
`accept_admin` to take the role, so an address that was mistyped or that nobody
holds never becomes the admin — which matters because `withdraw` is the only
non-claim path out of either vault.

**`claim_signer`** (`sleepagotchi_claim` only) — co-signs every claim. It is
never funded and holds no SOL. It cannot mint, cannot move a vault directly, and
cannot change any config. It can, however, authorize a payout of any size up to
the vault balance: there is no per-transaction or per-wallet ceiling, and one
was deliberately not added, because `claim(total)` takes a running total rather
than a delta and any such bound is sidestepped by walking the total upward or by
using fresh wallets. **The loss bound on a compromised claim signer is therefore
the vault balance**, which is why the vault is funded in tranches rather than in
full.

**`sleepagotchi_airdrop` has no signing key at all.** The published root is the
authorization. Nothing off-chain participates in a claim, so a claimant holding
their proof can claim while the backend is down.

**No key can redirect a payout.** Both programs constrain `destination` to the
claimant's own associated token account, so a proof or a co-signature only ever
pays the wallet it names.

## State

Both programs use a singleton `Config` at seeds `[b"config"]`, which is also the
authority on the vault. The vault is the config PDA's associated token account
for the mint — it has no seed of its own and is not stored, so there is no way
to pass the wrong one.

Receipts are at seeds `[b"receipt", user]` in both programs, and are never
closed.

| | `sleepagotchi_airdrop` | `sleepagotchi_claim` |
| --- | --- | --- |
| `Config` | `admin`, `pending_admin`, `mint`, `paused`, `bump` + `root`, `start_ts`, `end_ts` | `admin`, `pending_admin`, `mint`, `paused`, `bump` + `claim_signer` |
| `Receipt` | no fields — its existence is the claimed flag | `claimed: u64`, a running total of base units paid |

The asymmetry in `Receipt` is the design. An allocation is claimed once, so the
airdrop needs only a marker and gets replay protection from `init` failing on an
account that already exists. Entitlement in the claim program grows, so it needs
a running total, and `claim` takes the new total rather than a delta — a
resubmitted transaction then fails `NothingToClaim` instead of paying twice.

The airdrop receipt is seeded by wallet alone and never references the root, so
`set_root` cannot reopen a claim that has already been spent.

## Instructions

`sleepagotchi_airdrop`:

| Instruction | Signers | Notes |
| --- | --- | --- |
| `initialize(root, start_ts, end_ts)` | admin | Creates the config and the vault |
| `claim(amount, proof)` | user | The only path that moves tokens out other than `withdraw` |
| `set_root(root)` | admin | Invalidates every outstanding proof against the old root |
| `set_window(start_ts, end_ts)` | admin | Rejects an inverted window; may reopen a closed one |
| `transfer_admin(new_admin)` | admin | Proposes, or cancels with `None` |
| `accept_admin()` | pending admin | Completes the handover |
| `set_paused(paused)` | admin | Blocks claims, independent of the window |
| `withdraw(amount)` | admin | Unrestricted, at any time |

`sleepagotchi_claim`:

| Instruction | Signers | Notes |
| --- | --- | --- |
| `initialize(claim_signer)` | admin | Creates the config and the vault |
| `claim(total)` | user + claim_signer | `total` is the running total, not a delta |
| `set_claim_signer(key)` | admin | Takes effect immediately |
| `transfer_admin(new_admin)` | admin | Proposes, or cancels with `None` |
| `accept_admin()` | pending admin | Completes the handover |
| `set_paused(paused)` | admin | Blocks claims |
| `withdraw(amount)` | admin | Unrestricted, at any time |

Every instruction that moves tokens or changes the security posture emits an
event. `Claimed` carries both the delta paid and the receipt's new running total;
`ClaimSignerUpdated` and the two admin-transfer events are what a monitor should
alert on.

### The claim window

The airdrop is live over `[start_ts, end_ts)`, half-open, so the announced end
second is already closed. `NotStarted` and `Ended` are distinct errors because
they are different support conversations.

`paused` is separate from the window rather than folded into it: halting by
shrinking the window would overwrite the announced end date, and restoring it
afterwards would mean recovering it from memory under pressure.

`Clock::unix_timestamp` can drift by minutes, which is immaterial against a
window measured in days.

## The Merkle tree

Leaves are `keccak(0x00 || user || amount_le)`; internal nodes are
`keccak(0x01 || min || max)`. The distinct tags are what stop an internal node
being presented as a leaf. Pairs are sorted, so a proof carries no direction
bits, and the program derives the leaf itself from `user` and `amount` rather
than accepting one from the caller.

Proofs are capped at 24 hashes — 16.7M leaves, past any plausible tree.

The tree is also built in TypeScript, and the two implementations must agree
byte for byte. `fixtures/tree.json` is what holds them together: a Rust test
rebuilds every case and asserts the same root, depth and proof siblings. The
fixture is committed, so `cargo test` needs no Node toolchain.

Allocations are sorted by wallet address before building. Pairing is positional,
so without a canonical order the same set of wallets in a different order
produces a different root and nobody can reproduce it from the snapshot. One
leaf per wallet is enforced when the tree is built, not on chain — the receipt
is per wallet, so a wallet appearing twice would have one allocation silently
unclaimable.

## Build and test

See [README.md](README.md) — one `anchor build` covers all three programs.
