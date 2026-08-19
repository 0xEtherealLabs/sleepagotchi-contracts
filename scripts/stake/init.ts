/**
 * Initializes the program on a cluster and writes the deployment artifact.
 *
 *   pnpm init:devnet
 *
 * Deploy the bytecode first — `anchor program deploy --provider.cluster devnet` —
 * this only creates the config PDA. Kept separate because the two have different
 * idempotency: a deploy re-runs as an upgrade, `initialize` cannot run twice.
 *
 * No season exists afterwards. `pnpm season:devnet` opens one.
 */

import { Connection, PublicKey } from "@solana/web3.js";

import {
  type Deployment,
  assertWritable,
  configPda,
  deploymentPath,
  loadDeployer,
  loadOrCreateStakeSigner,
  parseCluster,
  send,
  writeDeployment,
} from "./common.js";
import { CLUSTERS } from "./config.js";
import { PROGRAM, instruction } from "./encode.js";

const cluster = parseCluster();
assertWritable(cluster);

const { rpcUrl, mint: mintAddress } = CLUSTERS[cluster];
const mint = new PublicKey(mintAddress);
const connection = new Connection(rpcUrl, "confirmed");
const admin = loadDeployer();
const stakeSigner = loadOrCreateStakeSigner(cluster);
const config = configPda();

if (!(await connection.getAccountInfo(PROGRAM))) {
  throw new Error(
    `Program ${PROGRAM.toBase58()} is not deployed on ${cluster}. Run \`anchor program deploy --provider.cluster ${cluster}\` first.`,
  );
}

if (await connection.getAccountInfo(config)) {
  throw new Error(
    `Already initialized on ${cluster} (config ${config.toBase58()}). Use the admin instructions to change it.`,
  );
}

const initialize = instruction(
  "initialize",
  { admin: admin.publicKey, config, mint },
  stakeSigner.publicKey.toBuffer(),
);

console.log(`initializing sleepagotchi_stake on ${cluster}`);
console.log(`  program      ${PROGRAM.toBase58()}`);
console.log(`  admin        ${admin.publicKey.toBase58()}`);
console.log(`  mint         ${mint.toBase58()}`);
console.log(`  stake signer ${stakeSigner.publicKey.toBase58()}`);

const signature = await send(connection, [admin], [initialize]);

const deployment: Deployment = {
  cluster,
  mint: mint.toBase58(),
  admin: admin.publicKey.toBase58(),
  programId: PROGRAM.toBase58(),
  config: config.toBase58(),
  stakeSigner: stakeSigner.publicKey.toBase58(),
  deployedAt: new Date().toISOString(),
  transactions: { initialize: signature },
};

writeDeployment(cluster, deployment);
console.log(`\nwrote ${deploymentPath(cluster)}`);
console.log("add the cluster to deployments/index.ts, then `pnpm season:devnet`");
