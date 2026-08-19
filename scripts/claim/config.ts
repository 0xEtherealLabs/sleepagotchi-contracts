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
    // SPL Token, which is what both programs constrain to.
    //
    // Fixed for the life of the programs: `Config` stores the mint at `initialize`
    // and nothing can change it afterwards, so a different mint here means
    // redeploying under new program ids. See §5 of the runbook.
    mint: "DADkWa3Vu6XhMSiAPNUCD32f1cM4gzCDANcijUMgsswu",
    blockedReason: null,
  },

  "mainnet-beta": {
    rpcUrl: "https://api.mainnet-beta.solana.com",
    mint: "",
    blockedReason:
      "Not configured for production — the $SLEEP mint, the Squads admin, the claim signer and the airdrop window are all TBC, and neither program has been audited. See §7 and §9 of the scope.",
  },
};

/**
 * Devnet claim window: opens immediately, closes in a year. A real window is a
 * decision with comms attached and is set with `set_window` before launch.
 */
export const DEVNET_WINDOW_SECONDS = 365 * 24 * 60 * 60;
