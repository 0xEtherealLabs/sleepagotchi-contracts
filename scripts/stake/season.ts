/**
 * Opens a season and funds its reward pool.
 *
 *   pnpm stake:season:devnet                    # the default in config.ts
 *   pnpm stake:season:devnet -- --seconds 600   # short, so a full claim can be rehearsed
 *   pnpm stake:season:devnet -- --dry-run
 *
 * The pool moves out of the deployer's own token account in the same
 * transaction, so `reward_vault.amount == reward_pool` holds from the moment the
 * season exists.
 *
 * Prints the two figures from §3 of the scope before signing: the most the season
 * can ever pay, and the point above which the pro-rata split starts to bind. A
 * pool above the first can never be claimed in full by anyone.
 */

import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { Connection, PublicKey } from "@solana/web3.js";

import {
  assertWritable,
  configPda,
  iso,
  loadDeployer,
  maxLiability,
  parseCluster,
  readConfig,
  readDeployment,
  readSeason,
  rewardsAuthorityPda,
  seasonPda,
  send,
} from "./common.js";
import { CLUSTERS, DEVNET_SEASON } from "./config.js";
import { instruction, seasonParams } from "./encode.js";

const cluster = parseCluster();
assertWritable(cluster);

const flag = (name: string): string | undefined => {
  const index = process.argv.indexOf(`--${name}`);

  return index === -1 ? undefined : process.argv[index + 1];
};
const dryRun = process.argv.includes("--dry-run");

const duration = BigInt(flag("seconds") ?? DEVNET_SEASON.durationSeconds);
if (duration <= 0n) {
  throw new Error("--seconds must be positive");
}

const deployment = readDeployment(cluster);
const { rpcUrl } = CLUSTERS[cluster];
const connection = new Connection(rpcUrl, "confirmed");
const admin = loadDeployer();
const mint = new PublicKey(deployment.mint);
const config = configPda();

const account = await connection.getAccountInfo(config);
if (!account) {
  throw new Error(`No config on ${cluster} — run \`pnpm stake:init:${cluster}\` first.`);
}
const current = readConfig(account.data);
if (!current.admin.equals(admin.publicKey)) {
  throw new Error(
    `${admin.publicKey.toBase58()} is not the admin — the config names ${current.admin.toBase58()}.`,
  );
}

const id = current.nextSeason;
const season = seasonPda(id);
const rewardsAuthority = rewardsAuthorityPda(id);
const stakeVault = getAssociatedTokenAddressSync(mint, season, true);
const rewardVault = getAssociatedTokenAddressSync(mint, rewardsAuthority, true);
const funding = getAssociatedTokenAddressSync(mint, admin.publicKey);

// A few seconds of headroom: `start_ts` must not be in the past by the time the
// transaction lands, and it is validated against the cluster's clock, not ours.
const earliest = BigInt(Math.floor(Date.now() / 1000) + 30);

/**
 * Seasons do not overlap, so a new one starts no earlier than the last one ends.
 *
 * Back to back rather than merely disjoint, because the gap between them is a
 * window in which nobody can roll a position forward — `stake` needs the next
 * season already open. Enforced here rather than in the program: overlapping
 * seasons are an economic leak (a wallet splitting its balance earns two APR
 * caps at once), not a safety bug, and the cost on chain is an extra account on
 * the one instruction that creates vaults.
 */
let previousEnd = 0n;
let startTs = earliest;
if (id > 0n) {
  const previousAccount = await connection.getAccountInfo(seasonPda(id - 1n));
  if (!previousAccount) {
    throw new Error(`Season ${id - 1n} is missing; refusing to guess where this one starts.`);
  }
  const previous = readSeason(previousAccount.data);
  previousEnd = previous.endTs;
  if (previousEnd > startTs) {
    startTs = previousEnd;
    console.log(`starting at season ${id - 1n}'s close, ${iso(startTs)} — seasons do not overlap`);
  }
}

const overrideStart = flag("start");
if (overrideStart) {
  startTs = BigInt(overrideStart);
  if (startTs < earliest) {
    throw new Error("--start is in the past");
  }
  // The default above already lands at or after the previous close; an override
  // has to clear the same bar. The program does not enforce this, so a season
  // opened inside another one is an economic leak nothing on chain will catch:
  // a wallet splitting its balance accrues in both and is capped in both.
  if (startTs < previousEnd) {
    throw new Error(
      `--start ${iso(startTs)} is before season ${id - 1n} closes at ${iso(previousEnd)}. ` +
        `Overlapping seasons let one wallet earn two APR caps at once.`,
    );
  }
}
const params = {
  startTs,
  endTs: startTs + duration,
  rewardPool: DEVNET_SEASON.rewardPool,
  maxTotalStaked: DEVNET_SEASON.maxTotalStaked,
  maxPerWallet: DEVNET_SEASON.maxPerWallet,
  maxAprBps: DEVNET_SEASON.maxAprBps,
  maxMultiplierBps: DEVNET_SEASON.maxMultiplierBps,
  sweepDelaySeconds: DEVNET_SEASON.sweepDelaySeconds,
};

const ceiling = maxLiability(params.maxTotalStaked, params.maxAprBps, duration);
// Staked amount at which the sum of every wallet's cap reaches the pool, which
// is where the split takes over from the cap as the binding constraint.
const crossover =
  ceiling === 0n ? 0n : (params.rewardPool * params.maxTotalStaked) / ceiling;

console.log(`opening season ${id} on ${cluster}`);
console.log(`  window        ${iso(params.startTs)} -> ${iso(params.endTs)}`);
console.log(`  reward pool   ${params.rewardPool}`);
console.log(`  max staked    ${params.maxTotalStaked}`);
console.log(`  max / wallet  ${params.maxPerWallet}`);
console.log(`  max APR       ${params.maxAprBps / 100}%`);
console.log(`  max weighting ${params.maxMultiplierBps / 10_000}x`);
console.log(
  `  claim window  ${params.sweepDelaySeconds / 86_400} days after close, before the remainder can be swept`,
);
console.log(`  season        ${season.toBase58()}`);
console.log(`  stake vault   ${stakeVault.toBase58()}`);
console.log(`  reward vault  ${rewardVault.toBase58()}`);
console.log();
console.log(`  most this season can ever pay   ${ceiling}`);
if (params.rewardPool > ceiling) {
  console.log(
    `  NOTE: the pool is larger than that, so ${params.rewardPool - ceiling} of it can never be claimed`,
  );
  console.log("        and the multiplier will not change anyone's payout.");
} else {
  console.log(`  the split starts to bind above  ${crossover} staked`);
}

if (dryRun) {
  console.log("\n--dry-run: signing nothing");
  process.exit(0);
}

const open = instruction(
  "open_season",
  {
    admin: admin.publicKey,
    config,
    season,
    rewards_authority: rewardsAuthority,
    mint,
    stake_vault: stakeVault,
    reward_vault: rewardVault,
    funding,
  },
  seasonParams(params),
);

console.log();
await send(connection, [admin], [open]);
console.log(`\nseason ${id} is open. \`pnpm stake:harness:${cluster}\` to read it back.`);
