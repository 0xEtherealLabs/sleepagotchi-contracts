export type ClusterName = "devnet" | "mainnet-beta";

export interface ClusterConfig {
  rpcUrl: string;
  /** The $SLEEP mint. */
  mint: string;
  /** Non-null blocks every write to this cluster, and says why. */
  blockedReason: string | null;
}

export const CLUSTERS: Record<ClusterName, ClusterConfig> = {
  devnet: {
    rpcUrl: "https://api.devnet.solana.com",
    // Sleepagotchi / SLEEP from `contracts/token/deployments/devnet.json`. Legacy
    // SPL Token, which is what the program constrains to.
    //
    // Fixed for the life of the program: `Config` stores the mint at `initialize`
    // and nothing can change it afterwards. See §5 of the runbook.
    mint: "DADkWa3Vu6XhMSiAPNUCD32f1cM4gzCDANcijUMgsswu",
    blockedReason: null,
  },

  "mainnet-beta": {
    rpcUrl: "https://api.mainnet-beta.solana.com",
    mint: "",
    blockedReason:
      "Not configured for production — the $SLEEP mint, the Squads admin, the stake signer and every season parameter are TBC, and the program has not been audited. See §11 and §13 of the scope.",
  },
};

/**
 * The season `pnpm season:devnet` opens with no arguments.
 *
 * Deliberately not the shape of a real season: it starts immediately so there is
 * nothing to wait for, and runs a week so the whole lifecycle can be exercised
 * without warping a clock nobody controls. Real parameters are a decision with
 * comms attached — see §13 of the scope.
 */
export const DEVNET_SEASON = {
  durationSeconds: 7 * 24 * 60 * 60,
  /** 1,000 tokens at nine decimals. Small: it is spent on rehearsals. */
  rewardPool: 1_000_000_000_000n,
  maxTotalStaked: 1_000_000_000_000_000n,
  maxPerWallet: 100_000_000_000_000n,
  maxAprBps: 1_500,
  /**
   * 1x — the floor and the ceiling, so the only weighting the program will
   * accept is none.
   *
   * The multiplier is not in use. Setting the ceiling to the floor makes that
   * structural rather than a promise the backend keeps: with these two equal,
   * a compromised or buggy stake signer cannot weight a position either. Raise
   * it here when a season is meant to use weighting, and fill in the tiers in
   * `src/config/stake-pool.config.ts` to match.
   */
  maxMultiplierBps: 10_000,

  /**
   * How long after a season closes the unclaimed remainder can be swept.
   *
   * A sweep takes the whole reward vault, including rewards earned but not yet
   * collected, so this is the window stakers actually have to claim in. The
   * program enforces a seven-day floor; this is the value devnet seasons open
   * with, and it is frozen once the season starts.
   */
  sweepDelaySeconds: 30 * 24 * 60 * 60,
} as const;
