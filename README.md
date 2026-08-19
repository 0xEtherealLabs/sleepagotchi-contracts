# Contracts

One Anchor workspace, three programs, one `anchor build`.

| Program | Pays for | Detail |
| --- | --- | --- |
| `sleepagotchi_airdrop` | A fixed allocation list, published as a Merkle root | [claim-programs.md](claim-programs.md) |
| `sleepagotchi_claim` | Amounts only known at request time, via a backend co-signature | [claim-programs.md](claim-programs.md) |
| `sleepagotchi_stake` | Seasonal staking rewards, pro-rata by stake over time | [staking-program.md](staking-program.md) |

[AUDIT-BRIEF.md](AUDIT-BRIEF.md) is the handover document for external review:
trust model, invariants, and the design decisions that are deliberate.

| Program | Id |
| --- | --- |
| `sleepagotchi_airdrop` | `DM6GCMdgn9daiiUCCaiCjrQHWwe3RiNJK3EPcqa2sdBg` |
| `sleepagotchi_claim` | `8igHauFjDNKpTkyATw9cigy7XWxWMbvHotA4S9i4qe1f` |
| `sleepagotchi_stake` | `AbXC8pN6zbyoi3qWxLcMQC3yXhNUHmXpuDJ1bVDadjKG` |

## One workspace, three programs

Isolation between the programs comes from distinct program ids, distinct vault
PDAs, and distinct upgrade authorities — none of which is a property of the
workspace. Sharing a workspace shares a toolchain, a lockfile and a test
harness; it does not share an address, a vault or an authority.

No program depends on another, and there is no shared crate. A finding in one is
a finding in one.

## Build and test

Anchor 1.1.2 via avm, installed from git rather than crates.io:

```bash
cargo install --git https://github.com/solana-foundation/anchor avm --locked
```

```bash
avm install 1.1.2 && avm use 1.1.2
```

The `avm` crate on crates.io is an unrelated, abandoned Node version manager that
fails to build against modern OpenSSL. Anchor 1.1.2 bundles its own Solana
3.1.10, so the system `solana-cli` version does not affect `anchor build`.

```bash
anchor build && cargo test
```

Tests are LiteSVM, in process, and link the built programs from
`target/deploy/`, so a bare `cargo test` on a clean tree will not compile until
`anchor build` has run once.

The offline layout check validates the hand-rolled Borsh encoders in `scripts/`
against the committed IDL. Run it before any deploy:

```bash
pnpm check
```

Node dependencies for `scripts/` and `token/` (Node 22.9 or newer):

```bash
pnpm run setup
```

`--ignore-workspace`, because this repo is normally checked out as a submodule of
the app, whose `pnpm-workspace.yaml` would otherwise claim the install.

Copy `.env.example` to `.env` if you need it. Nothing but
`pnpm token:upload:<cluster>` requires a value in it — everything else takes the
Solana CLI's own keypair by default.

## Layout

| Path | What |
| --- | --- |
| `programs/<name>/` | One crate per program: `constants`, `state`, `error`, `events`, `instructions/<name>.rs`, and `tests/` |
| `idl/` | Committed IDL and TypeScript types; the app builds transactions from these |
| `fixtures/` | Cross-implementation fixtures binding the Rust and TypeScript halves |
| `deployments/claim/`, `deployments/stake/` | Per-cluster addresses, written on deploy |
| `scripts/claim/`, `scripts/stake/` | Deploy, configuration and devnet harnesses |
| `token/` | Deployment scripts for the SPL mint, and the image that is its identity. No program. [token/README.md](token/README.md) |
| `lib/merkle.ts` | The airdrop Merkle tree. One implementation, three consumers |

`scripts/` is namespaced per program group because both halves define their own
`common.ts`, `config.ts` and `encode.ts` against different layouts. They are
deliberately not merged: the encoders are a second copy of each program's account
layout, and one shared module would make a change to either a change to both.

`idl/` is committed rather than treated as a build artifact, so a change to any
program's surface appears as a reviewed diff. `Cargo.lock` is committed because
`solana-verify` needs it for a reproducible build.

## The app imports this repo

The Sleepagotchi app checks this out as a submodule at `contracts/` and imports
four things directly, so their shapes are its build surface, not just ours:

| Path | Why the app reads it |
| --- | --- |
| `idl/*.json` | Builds and decodes every transaction from it |
| `deployments/claim/`, `deployments/stake/`, `token/deployments/` | Which addresses exist on which cluster |
| `token/config.ts` | Decimals, supply, cluster names — the app formats balances with them |
| `lib/merkle.ts` | Serves airdrop proofs from the same tree the fixture pins |

Those files are typechecked twice, here and there, under different module
resolutions — which is why `tsconfig.json` uses `bundler` rather than `nodenext`.
Renaming or moving one of them is an app change as much as a contracts change.

## Program keypairs are not in git

`target/` is gitignored, and that is where `anchor build` puts
`<name>-keypair.json`. Each fixes its program's address. Back them up outside the
repo; after the first deploy the upgrade authority is what matters instead.
