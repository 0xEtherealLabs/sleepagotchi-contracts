/**
 * Sets the airdrop claim window.
 *
 *   pnpm claim:window:devnet -- --start 2026-09-08T12:00:00Z --end 2026-12-08T12:00:00Z
 *   pnpm claim:window:devnet -- --start now --end 2027-09-08T12:00:00Z
 *
 * The program enforces only that the start precedes the end. Everything else
 * checked here is a way to close a window by accident — and a window that has
 * already ended cannot be reopened by anyone who has since lost the admin key.
 *
 * The app reads the window from the config rather than from its own settings, so
 * this is the only place it is decided. Nothing needs redeploying afterwards.
 */

import { Connection, PublicKey } from "@solana/web3.js";

import {
  adminInstruction,
  parseCluster,
  assertWritable,
  iso,
  loadDeployer,
  PROGRAMS,
  readAirdropConfig,
  readDeployment,
  send,
  writeDeployment,
} from "./common.js";
import { CLUSTERS } from "./config.js";
import { IDLS, Reader, discriminator, i64 } from "./encode.js";

const flag = (name: string): string | undefined => {
  const at = process.argv.indexOf(`--${name}`);
  return at === -1 ? undefined : process.argv[at + 1];
};

/** `now` is the only shorthand, because it is the only one worth a bug. */
const seconds = (value: string, label: string): number => {
  if (value === "now") return Math.floor(Date.now() / 1000);

  const at = Date.parse(value);
  if (Number.isNaN(at)) {
    throw new Error(`--${label} "${value}" is not a date. Use an ISO timestamp, or "now".`);
  }

  return Math.floor(at / 1000);
};

const cluster = parseCluster();
const force = process.argv.includes("--force");
const dryRun = process.argv.includes("--dry-run");

const start = flag("start");
const end = flag("end");

if (!start || !end) {
  throw new Error(
    'usage: pnpm claim:window:<cluster> -- --start <iso|now> --end <iso> [--dry-run] [--force]',
  );
}

if (!dryRun) assertWritable(cluster);

const startTs = seconds(start, "start");
const endTs = seconds(end, "end");
const now = Math.floor(Date.now() / 1000);

if (startTs >= endTs) {
  throw new Error(
    `the window ends before it begins: ${iso(startTs)} -> ${iso(endTs)}.`,
  );
}

// A window that is already over the moment it is set. The program accepts it —
// it only compares the two — and the result is an airdrop nobody can ever claim.
if (endTs <= now && !force) {
  throw new Error(
    `that window ended at ${iso(endTs)}, which is in the past. Nothing would ever be claimable — pass --force if you are deliberately closing the airdrop.`,
  );
}

const connection = new Connection(CLUSTERS[cluster].rpcUrl, "confirmed");
const deployment = readDeployment(cluster);
const admin = loadDeployer();

const configAddress = new PublicKey(deployment.airdrop.config);
const account = await connection.getAccountInfo(configAddress);
if (!account) {
  throw new Error(
    `airdrop config ${configAddress.toBase58()} does not exist — run \`pnpm claim:init:${cluster}\` first.`,
  );
}

const config = readAirdropConfig(account.data);
const onChainAdmin = config.admin;
const currentStart = config.startTs;
const currentEnd = config.endTs;

if (!onChainAdmin.equals(admin.publicKey)) {
  throw new Error(
    `${admin.publicKey.toBase58()} is not the airdrop admin — the config says ${onChainAdmin.toBase58()}.`,
  );
}

console.log(`cluster  ${cluster}`);
console.log(`current  ${iso(currentStart)} -> ${iso(currentEnd)}`);
console.log(`setting  ${iso(startTs)} -> ${iso(endTs)}`);

// Shortening a window that people are already claiming in. Not wrong, but it is
// the sort of thing that should be a decision rather than a consequence.
if (currentStart <= now && now < currentEnd && endTs < currentEnd && !force) {
  throw new Error(
    `the airdrop is open now and this brings the close forward from ${iso(currentEnd)} to ${iso(endTs)} — pass --force if that is deliberate.`,
  );
}

if (dryRun) {
  console.log("\ndry run — nothing sent.");
  process.exit(0);
}

console.log("\nsetting the window");
const signature = await send(connection, admin, [
  adminInstruction(
    PROGRAMS.airdrop,
    admin.publicKey,
    Buffer.concat([
      discriminator(IDLS.airdrop, "set_window"),
      i64(BigInt(startTs)),
      i64(BigInt(endTs)),
    ]),
  ),
]);

deployment.airdrop.startTs = startTs;
deployment.airdrop.endTs = endTs;
deployment.transactions.setWindow = signature;
writeDeployment(cluster, deployment);

console.log(
  startTs <= now
    ? "\nopen now."
    : `\nopens ${iso(startTs)}.`,
);
