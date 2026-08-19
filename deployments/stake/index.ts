/**
 * The deployment artifacts, as a module the app can import — same shape and same
 * rules as `contracts/deployments/claim`.
 *
 * A network appears here only once its deploy has happened and been committed.
 * Adding the line is manual on purpose, so a vault reaching the app is a reviewed
 * diff rather than whatever JSON happens to be on disk.
 *
 * `scripts/common.ts` reads the same files off disk, which is fine for a script
 * and impossible in a browser bundle. Same files, different door.
 */
export type ClusterName = "devnet" | "mainnet-beta";

export interface Deployment {
  cluster: ClusterName;
  mint: string;
  admin: string;
  programId: string;
  config: string;
  /** Public half only. The secret lives in `STAKE_SIGNER_SECRET_KEY`. */
  stakeSigner: string;
  deployedAt: string;
  transactions: Record<string, string>;
}

import devnet from "./devnet.json";

export const DEPLOYMENTS: Partial<Record<ClusterName, Deployment>> = {
  // Written by `pnpm stake:init`. The cast is the JSON import losing string
  // literals — `cluster: "devnet"` widens to `string`.
  devnet: devnet as Deployment,
};
