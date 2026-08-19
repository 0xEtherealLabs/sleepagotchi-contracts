# Token

A plain SPL Token mint. No program, nothing to upgrade, nothing to audit.

Listed here because all three programs assume its properties: legacy SPL Token
rather than Token-2022, fixed supply, and no transfer hooks or fees. Their
account structs use `Program<Token>` and `TokenAccount` throughout, never
`token_interface`, so a Token-2022 mint cannot be substituted — which is what
keeps every vault's balance exactly what the programs' own accounting says it
is.

Full supply is minted once at deploy, after which mint and freeze authorities
are revoked.

Devnet used to deploy as **Napagotchi / NAPS** so a test token could not be
mistaken for the real one. It is now **Sleepagotchi / SLEEP** on both clusters,
for client demos — which means a screenshot of a balance no longer tells you
which network it came from. The cluster badge in the app is the only thing that
does.

| File | What |
| --- | --- |
| [`config.ts`](config.ts) | Token parameters, per-network config, and the metadata document |
| [`deploy.ts`](deploy.ts) | Creates the mint |
| [`transfer.ts`](transfer.ts) | Sends tokens from the treasury |
| [`upload.ts`](upload.ts) | Pins the image and metadata document to IPFS |
| [`rename.ts`](rename.ts) | Rewrites name, symbol and uri on chain |
| [`verify.ts`](verify.ts) | Checks the account, the document and the image |
| `sleep-1024.png` | The image `upload.ts` pins. A copy of the app's `public/coin/Sleepa Coin 1024x1024.png` — replace both, or the token and the site stop matching |
| `deployments/<cluster>.json` | Written on deploy; the record of what shipped |

## Usage

```bash
pnpm token:deploy:devnet
```

```bash
pnpm token:transfer:devnet <recipient> <amount>
```

Amount is in whole tokens (`1000`, `12.5`).

## Identity

Name, symbol and image are metadata, not mint state, so they can change without
a new mint — `METADATA_IS_MUTABLE` is true and `deploy.ts` leaves the update
authority with the treasury. Changing them is three steps:

```bash
pnpm token:upload:devnet
```

Pins the image and the metadata document and prints a URL. Paste it into
`metadataUri` in `config.ts` — the chain should point at a reviewed constant,
not at whatever the last upload happened to produce.

```bash
pnpm token:rename:devnet
```

```bash
pnpm token:verify:devnet
```

`verify` is the test, not the wallet. Phantom, Solscan and Jupiter cache token
metadata for hours, so a correct rename routinely still looks wrong in a wallet
well after it landed. `verify` reads the account and fetches the document, which
is what those caches are a stale copy of.

The uri that goes on chain is an **HTTPS gateway URL, not `ipfs://`**. Solana
wallets fetch `uri` over plain HTTP and most do not dereference the ipfs scheme;
an `ipfs://` uri renders as a nameless, imageless token in exactly the places
that matter. Same for the `image` field inside the document — note that
`upload.ts` serialises the document itself for this reason, because thirdweb
rewrites gateway URLs back to `ipfs://` in any JSON object it is handed.

Deploying over an existing `deployments/*.json` is refused: a mint cannot be
redeployed in place, so a second run means a second, unrelated token. `--force`
overrides, which orphans the previous mint.

`SOLANA_DEPLOYER_KEYPAIR` defaults to the Solana CLI's
`~/.config/solana/id.json`, and only `upload.ts` needs anything else — the two
thirdweb keys in `.env.example`. Everything the token *is* lives in `config.ts`:
which address holds the supply happens once and irreversibly, so it belongs in a
reviewed diff rather than a per-machine `.env`.

`transfer.ts` refuses to run once `treasury` is set, because a local keypair
cannot sign for a multisig.

## Units

Balances are bigints in base units — no floats, no decimal strings. The
deployment artifact records supply as a string because JSON has no bigint.
