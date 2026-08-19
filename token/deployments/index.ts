import type { Artifact } from "../common";
import type { ClusterName } from "../config";

import devnet from "./devnet.json";

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
};
