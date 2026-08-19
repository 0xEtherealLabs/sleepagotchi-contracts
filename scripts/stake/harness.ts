/**
 * Reads the on-chain state. Read-only unless a write flag is passed.
 *
 *   pnpm harness:devnet                        # dump config, seasons, own position
 *   pnpm harness:devnet -- --stake 1000        # stake, co-signed by the devnet signer
 *   pnpm harness:devnet -- --unstake 1000
 *   pnpm harness:devnet -- --claim
 *   pnpm harness:devnet -- --sweep 0          # admin: take a closed season's remainder
 *
 * Amounts are whole tokens; they are scaled by the mint's decimals here.
 *
 * A live cluster proves things LiteSVM cannot: real rent, real ATA creation, the
 * real token program, and a clock nobody controls. Open a short season first —
 * `pnpm season:devnet -- --seconds 600` — or there is nothing to claim for a week.
 */

import {
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
  getMint,
} from "@solana/spl-token";
import { Connection, PublicKey, TransactionInstruction } from "@solana/web3.js";

import {
  assertWritable,
  configPda,
  iso,
  loadDeployer,
  loadOrCreateStakeSigner,
  parseCluster,
  positionPda,
  readConfig,
  readDeployment,
  readPosition,
  readSeason,
  rewardsAuthorityPda,
  seasonPda,
  send,
} from "./common.js";
import { CLUSTERS } from "./config.js";
import { PROGRAM, instruction, u32, u64 } from "./encode.js";

const cluster = parseCluster();
const deployment = readDeployment(cluster);
const { rpcUrl } = CLUSTERS[cluster];
const connection = new Connection(rpcUrl, "confirmed");
const admin = loadDeployer();
const mint = new PublicKey(deployment.mint);

const flag = (name: string): string | undefined => {
  const index = process.argv.indexOf(`--${name}`);

  return index === -1 ? undefined : process.argv[index + 1];
};

const stakeAmount = flag("stake");
const unstakeAmount = flag("unstake");
const claiming = process.argv.includes("--claim");
const sweeping = flag("sweep");
const multiplier = Number(flag("multiplier") ?? 10_000);

const config = configPda();
const account = await connection.getAccountInfo(config);
if (!account) {
  throw new Error(`No config on ${cluster} — run \`pnpm init:${cluster}\` first.`);
}
const current = readConfig(account.data);

console.log(`sleepagotchi_stake on ${cluster}`);
console.log(`  program       ${PROGRAM.toBase58()}`);
console.log(`  config        ${config.toBase58()}`);
console.log(`  admin         ${current.admin.toBase58()}`);
console.log(`  stake signer  ${current.stakeSigner.toBase58()}`);
console.log(`  mint          ${current.mint.toBase58()}`);
console.log(`  paused        ${current.paused}`);
console.log(`  seasons       ${current.nextSeason}`);

const decimals = (await getMint(connection, mint)).decimals;
const scale = 10n ** BigInt(decimals);
const now = Math.floor(Date.now() / 1000);

for (let id = 0n; id < current.nextSeason; id += 1n) {
  const address = seasonPda(id);
  const raw = await connection.getAccountInfo(address);
  if (!raw) {
    console.log(`\nseason ${id}: MISSING at ${address.toBase58()}`);
    continue;
  }

  const season = readSeason(raw.data);
  const live = now >= Number(season.startTs) && now < Number(season.endTs);
  const state = live ? "live" : now < Number(season.startTs) ? "pending" : "closed";

  console.log(`\nseason ${id} (${state})`);
  console.log(`  window        ${iso(season.startTs)} -> ${iso(season.endTs)}`);
  console.log(`  reward pool   ${season.rewardPool}`);
  console.log(`  paid so far   ${season.rewardsPaid}`);
  console.log(`  staked        ${season.totalStaked} / ${season.maxTotalStaked}`);
  console.log(`  weighted      ${season.totalWeighted}`);
  console.log(`  accrued (WSS) ${season.weightedStakeSeconds}`);
  console.log(`  settled to    ${iso(season.lastUpdateTs)}`);
  console.log(`  swept         ${season.swept}`);

  const stakeVault = getAssociatedTokenAddressSync(mint, address, true);
  const rewardVault = getAssociatedTokenAddressSync(mint, rewardsAuthorityPda(id), true);
  for (const [label, vault] of [
    ["stake vault ", stakeVault],
    ["reward vault", rewardVault],
  ] as const) {
    const balance = await connection.getTokenAccountBalance(vault).catch(() => null);
    console.log(`  ${label}  ${balance?.value.amount ?? "—"}  ${vault.toBase58()}`);
  }

  const position = await connection.getAccountInfo(positionPda(id, admin.publicKey));
  if (position) {
    const held = readPosition(position.data);
    console.log(`  own position  ${held.amount} at ${held.multiplierBps / 10_000}x`);
    console.log(`    accrued     ${held.weightedStakeSeconds} weighted`);
    console.log(`    raw         ${held.rawStakeSeconds}`);
    console.log(`    claimed     ${held.claimed}`);
  }
}

if (!stakeAmount && !unstakeAmount && !claiming && !sweeping) {
  console.log("\nread-only. --stake / --unstake / --claim to exercise it.");
  process.exit(0);
}

assertWritable(cluster);

/** The newest season that is currently accepting stakes. */
const liveSeason = async (): Promise<bigint> => {
  for (let id = current.nextSeason - 1n; id >= 0n; id -= 1n) {
    const raw = await connection.getAccountInfo(seasonPda(id));
    if (!raw) continue;
    const season = readSeason(raw.data);
    if (now >= Number(season.startTs) && now < Number(season.endTs)) return id;
  }

  throw new Error("No live season. Open one with `pnpm season:devnet`.");
};

const tokenAccount = getAssociatedTokenAddressSync(mint, admin.publicKey);

if (stakeAmount) {
  const id = await liveSeason();
  const amount = BigInt(stakeAmount) * scale;
  const stakeSigner = loadOrCreateStakeSigner(cluster);

  console.log(`\nstaking ${amount} into season ${id} at ${multiplier / 10_000}x`);
  const ix = instruction(
    "stake",
    {
      user: admin.publicKey,
      stake_signer: stakeSigner.publicKey,
      config,
      season: seasonPda(id),
      position: positionPda(id, admin.publicKey),
      mint,
      stake_vault: getAssociatedTokenAddressSync(mint, seasonPda(id), true),
      source: tokenAccount,
    },
    Buffer.concat([u64(amount), u32(multiplier)]),
  );

  await send(connection, [admin, stakeSigner], [ix]);
}

if (unstakeAmount) {
  const id = await liveSeason().catch(() => current.nextSeason - 1n);
  const amount = BigInt(unstakeAmount) * scale;

  console.log(`\nunstaking ${amount} from season ${id} — no co-signature, no window check`);
  const ix = instruction(
    "unstake",
    {
      user: admin.publicKey,
      config,
      season: seasonPda(id),
      position: positionPda(id, admin.publicKey),
      mint,
      stake_vault: getAssociatedTokenAddressSync(mint, seasonPda(id), true),
      destination: tokenAccount,
    },
    u64(amount),
  );

  await send(connection, [admin], [ix]);
}

if (claiming) {
  // The most recent closed season, which is the only kind that pays.
  let id = -1n;
  for (let candidate = current.nextSeason - 1n; candidate >= 0n; candidate -= 1n) {
    const raw = await connection.getAccountInfo(seasonPda(candidate));
    if (raw && now >= Number(readSeason(raw.data).endTs)) {
      id = candidate;
      break;
    }
  }
  if (id < 0n) {
    throw new Error("No closed season to claim from.");
  }

  console.log(`\nclaiming from season ${id} — no co-signature`);
  const instructions: TransactionInstruction[] = [
    createAssociatedTokenAccountIdempotentInstruction(
      admin.publicKey,
      tokenAccount,
      admin.publicKey,
      mint,
    ),
    instruction("claim", {
      user: admin.publicKey,
      config,
      season: seasonPda(id),
      position: positionPda(id, admin.publicKey),
      rewards_authority: rewardsAuthorityPda(id),
      mint,
      reward_vault: getAssociatedTokenAddressSync(mint, rewardsAuthorityPda(id), true),
      destination: tokenAccount,
    }),
  ];

  const before = await connection.getTokenAccountBalance(tokenAccount).catch(() => null);
  await send(connection, [admin], instructions);
  const after = await connection.getTokenAccountBalance(tokenAccount);
  console.log(
    `  received ${BigInt(after.value.amount) - BigInt(before?.value.amount ?? "0")}`,
  );
}

if (sweeping) {
  const id = BigInt(sweeping);
  console.log(`\nsweeping season ${id} to the treasury — admin only, any time after the close`);
  const ix = instruction("sweep_unclaimed", {
    admin: admin.publicKey,
    config,
    season: seasonPda(id),
    rewards_authority: rewardsAuthorityPda(id),
    mint,
    reward_vault: getAssociatedTokenAddressSync(mint, rewardsAuthorityPda(id), true),
    destination: tokenAccount,
  });

  const before = await connection.getTokenAccountBalance(tokenAccount);
  await send(connection, [admin], [ix]);
  const after = await connection.getTokenAccountBalance(tokenAccount);
  console.log(`  recovered ${BigInt(after.value.amount) - BigInt(before.value.amount)}`);
}
