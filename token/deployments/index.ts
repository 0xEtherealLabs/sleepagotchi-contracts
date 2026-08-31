import type { Artifact } from "../common";
import type { ClusterName } from "../config";

import devnet from "./devnet.json";
import mainnetBeta from "./mainnet-beta.json";

/**
 * The deployment artifacts, as a module the app can import.
 *
 * `common.ts` reads these off disk, which is fine for a script and impossible
 * in a browser bundle. Same files, different door.
 *
 * A network appears here only once its deploy has actually happened and been
 * committed — adding the line is manual on purpose, so a mint reaching the app
 * is a reviewed diff rather than whatever JSON happens to be on disk.
 */
export const DEPLOYMENTS: Partial<Record<ClusterName, Artifact>> = {
  // Written by `pnpm token:deploy:devnet`. The cast is the JSON import losing
  // string literals — `cluster: "devnet"` widens to `string`.
  devnet: devnet as Artifact,

  // Deployed 2026-08-21. Mint and freeze authorities are both revoked and the
  // treasury Squads vault holds the whole billion, so this artifact describes a
  // token that can no longer change supply.
  //
  // `metadataUri` is empty: the metadata account exists with the right name and
  // symbol but points at no document, so wallets render $SLEEP with no image and
  // no description. `METADATA_IS_MUTABLE` is true and the update authority is the
  // treasury, so fixing it is an upload plus a multisig-signed update — not a
  // redeploy.
  "mainnet-beta": mainnetBeta as Artifact,
};
