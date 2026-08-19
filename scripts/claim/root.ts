/**
 * Publishes the airdrop Merkle root.
 *
 *   pnpm root:devnet
 *   pnpm root:devnet -- --dry-run   # show what would change, send nothing
 *   pnpm root:devnet -- --force     # replace a root that is already published
 *
 * The root comes from `src/config/airdrop/root.json`, written by
 * `pnpm airdrop:snapshot` alongside the allocation list it hashes. Nothing is
 * accepted on the command line: the app serves proofs against the committed
 * list, so publishing anything else would leave every one of them failing on
 * chain.
 *
 * This is the instruction that makes an airdrop claimable. Until it runs the
 * config holds a zero root and nothing verifies.
 */

import { Connection, PublicKey } from "@solana/web3.js";

import {
  adminInstruction,
  parseCluster,
  assertWritable,
  loadDeployer,
  PROGRAMS,
  readAirdropConfig,
  readDeployment,
  readPublished,
  send,
  writeDeployment,
} from "./common.js";
import { CLUSTERS } from "./config.js";
import { IDLS, Reader, discriminator } from "./encode.js";

const cluster = parseCluster();
const force = process.argv.includes("--force");
const dryRun = process.argv.includes("--dry-run");

if (!dryRun) assertWritable(cluster);

const connection = new Connection(CLUSTERS[cluster].rpcUrl, "confirmed");
const deployment = readDeployment(cluster);
const admin = loadDeployer();
const published = readPublished();

const configAddress = new PublicKey(deployment.airdrop.config);
const account = await connection.getAccountInfo(configAddress);
if (!account) {
  throw new Error(
    `airdrop config ${configAddress.toBase58()} does not exist — run \`pnpm init:${cluster}\` first.`,
  );
}

const config = readAirdropConfig(account.data);
const onChainAdmin = config.admin;
const current = config.root;

if (!onChainAdmin.equals(admin.publicKey)) {
  throw new Error(
    `${admin.publicKey.toBase58()} is not the airdrop admin — the config says ${onChainAdmin.toBase58()}.`,
  );
}

console.log(`cluster    ${cluster}`);
console.log(`config     ${configAddress.toBase58()}`);
console.log(`current    ${current}${/^0+$/.test(current) ? "  (unset)" : ""}`);
console.log(`publishing ${published.root}`);
console.log(`claimants  ${published.claimants}`);
console.log(`total      ${published.totalBaseUnits} base units`);

if (current === published.root) {
  console.log("\nalready published — nothing to do.");
  process.exit(0);
}

/**
 * Replacing a live root is not a correction, it is a different airdrop. Every
 * proof already handed out stops verifying, and anyone mid-claim fails. It is
 * occasionally the right thing to do, and never the thing to do by accident.
 */
if (!/^0+$/.test(current) && !force) {
  throw new Error(
    `${cluster} already has a root published. Replacing it invalidates every proof already served and breaks any claim in flight — pass --force if that is what you mean.`,
  );
}

if (dryRun) {
  console.log("\ndry run — nothing sent.");
  process.exit(0);
}

console.log("\npublishing");
const signature = await send(connection, admin, [
  adminInstruction(
    PROGRAMS.airdrop,
    admin.publicKey,
    Buffer.concat([
      discriminator(IDLS.airdrop, "set_root"),
      Buffer.from(published.root, "hex"),
    ]),
  ),
]);

// The artifact is what the app reads for addresses, and a stale root in it is a
// small lie that costs someone an afternoon.
deployment.airdrop.root = published.root;
deployment.transactions.setRoot = signature;
writeDeployment(cluster, deployment);

console.log("\nroot published. fund the vault next:");
console.log(`  pnpm fund:${cluster}`);
