export type ClusterName = "devnet" | "mainnet-beta";

/** Same on every network, so devnet is a real rehearsal of mainnet. */
export const TOKEN_DECIMALS = 9;
export const TOTAL_SUPPLY = 1_000_000_000n;
export const TOTAL_SUPPLY_BASE_UNITS =
  TOTAL_SUPPLY * 10n ** BigInt(TOKEN_DECIMALS);

/** The mint authority is revoked regardless; this only covers name/symbol/uri. */
export const METADATA_IS_MUTABLE = true;

export type ClusterConfig = {
  rpcUrl: string;
  name: string;
  symbol: string;
  /**
   * The document a wallet fetches to get the description and image. Written by
   * `pnpm token:upload:<cluster>` and pasted back here, so what the chain points
   * at is a reviewed constant rather than whatever the last upload produced.
   */
  metadataUri: string;
  /** Squads multisig. `null` means the deployer holds the supply. */
  treasury: string | null;
  /** Non-null blocks deploys to this cluster, and says why. */
  blockedReason: string | null;
};

/**
 * The off-chain metadata document, minus the parts that differ per cluster.
 *
 * Shared deliberately: the image and the description are the token's identity,
 * and a devnet build that described itself differently would be a rehearsal of
 * something else. Only `metadataUri` is per-cluster, because each cluster's
 * document is uploaded separately and lands on its own CID.
 */
export const METADATA = {
  description:
    "Sleepagotchi is the app that pays you to sleep better. SLEEP connects users, AI agents, wearables and wellness partners — and rewards you for sleeping well.",
  externalUrl: "https://sleepagotchi.com",
  /**
   * Repo-relative. The official 1024px render — square and transparent. Wallets
   * downscale, so the ceiling is quality rather than layout.
   *
   * A copy of the app's `public/coin/Sleepa Coin 1024x1024.png`. What a wallet
   * shows is whatever `pnpm token:upload:<cluster>` pinned from *this* file, so
   * replacing the app's render is not enough — replace both, or the token and
   * the site stop matching.
   */
  image: "token/sleep-1024.png",
} as const;

export const CLUSTERS: Record<ClusterName, ClusterConfig> = {
  devnet: {
    rpcUrl: "https://api.devnet.solana.com",
    /*
     * Identical to mainnet, on purpose, so the client demo shows the real thing.
     *
     * This gives up a safety property that was deliberate: devnet used to deploy
     * as Napagotchi / NAPS precisely so a test token could never be mistaken for
     * $SLEEP in a wallet or a screenshot. It now can be. The cluster badge in the
     * app is the only thing left that distinguishes them, so treat any screenshot
     * of a balance as ambiguous unless it shows one.
     */
    name: "Sleepagotchi",
    symbol: "SLEEP",
    metadataUri:
      "https://73e3481d9d95554e14aeda900f0a3c88.ipfscdn.io/ipfs/QmSHUuZHcZujAUnZB9yqnAVjWwUbZgsTmgucgoJRt2tCQX",
    treasury: null,
    blockedReason: null,
  },

  "mainnet-beta": {
    /*
     * Public endpoint only, and deliberately so — never paste a keyed URL here.
     *
     * The app imports `CLUSTERS` into `src/config/token.config.ts`, which client
     * components import, so every field of every entry is emitted verbatim into
     * the browser bundle regardless of which cluster the build selects. A URL
     * with an API key in it is a published API key.
     *
     * The deploy scripts need something better than this — the public endpoint
     * rate-limits a mint hard enough to matter — so they read `SOLANA_RPC_URL`
     * from the environment and fall back to this. The app's server does the same
     * in `src/services/rpc.service.ts`. Put the keyed provider URL there.
     */
    rpcUrl: "https://api.mainnet-beta.solana.com",
    name: "Sleepagotchi",
    symbol: "SLEEP",
    metadataUri: "",
    treasury: "5aZLE8mpDXQXLaayWtV52P4oTZLPsfkJjk37N2a1mfYU",
    blockedReason: null,
  },
};
