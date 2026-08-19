/**
 * The deployment artifacts, as a module the app can import — same shape and same
 * rules as `contracts/token/deployments`.
 *
 * A network appears here only once its deploy has happened and been committed.
 * Adding the line is manual on purpose, so a vault reaching the app is a reviewed
 * diff rather than whatever JSON happens to be on disk.
 *
 * `scripts/common.ts` reads the same files off disk, which is fine for a script
 * and impossible in a browser bundle. Same files, different door.
 */
export type ClusterName = "devnet" | "mainnet-beta";

export interface AirdropDeployment {
  programId: string;
  config: string;
  vault: string;
  /** Hex. Zero means no tree published yet, so nothing is claimable. */
  root: string;
  /** Unix seconds. Claims are live over `[startTs, endTs)`. */
  startTs: number;
  endTs: number;
}

export interface SignedClaimDeployment {
  programId: string;
  config: string;
  vault: string;
  /** Public half only. The secret lives in `CLAIM_SIGNER_SECRET_KEY`. */
  claimSigner: string;
}

export interface Deployment {
  cluster: ClusterName;
  mint: string;
  admin: string;
  deployedAt: string;
  airdrop: AirdropDeployment;
  claim: SignedClaimDeployment;
  transactions: Record<string, string>;
}

import devnet from "./devnet.json";

export const DEPLOYMENTS: Partial<Record<ClusterName, Deployment>> = {
  // Written by `pnpm init:devnet`. The cast is the JSON import losing string
  // literals — `cluster: "devnet"` widens to `string`.
  devnet: devnet as Deployment,
};
