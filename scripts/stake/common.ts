import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";

import {
  Connection,
  Keypair,
  PublicKey,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";

import { CLUSTERS, type ClusterName } from "./config.js";
import { PROGRAM, Reader } from "./encode.js";

export const CONFIG_SEED = Buffer.from("config");
export const SEASON_SEED = Buffer.from("season");
export const REWARDS_SEED = Buffer.from("rewards");
export const POSITION_SEED = Buffer.from("position");

export function parseCluster(): ClusterName {
  const name = process.argv[2];
  if (!name || !(name in CLUSTERS)) {
    throw new Error(`Pass a cluster: ${Object.keys(CLUSTERS).join(" | ")}`);
  }

  return name as ClusterName;
}

/** Throws on a cluster that is not configured for writes. Call before signing. */
export function assertWritable(cluster: ClusterName): void {
  const { blockedReason } = CLUSTERS[cluster];
  if (blockedReason) {
    throw new Error(`Refusing to write to ${cluster}: ${blockedReason}`);
  }
}

/**
 * Same convention as `contracts/scripts/claim/common.ts` — a path to a Solana CLI
 * keypair, which is a JSON array of the 64 secret key bytes.
 */
export function loadDeployer(): Keypair {
  const path = process.env.SOLANA_DEPLOYER_KEYPAIR ?? "~/.config/solana/id.json";
  const expanded = path.startsWith("~/") ? resolve(homedir(), path.slice(2)) : resolve(path);

  try {
    return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(expanded, "utf8"))));
  } catch {
    throw new Error(
      `No usable Solana keypair at ${expanded}. Create one with \`solana-keygen new\`, or set SOLANA_DEPLOYER_KEYPAIR.`,
    );
  }
}

/**
 * The devnet stake signer. Generated on first use and kept out of git — it stands
 * in for whatever the backend holds via `STAKE_SIGNER_SECRET_KEY`, and is never
 * funded, so losing it costs a `set_stake_signer` and nothing else.
 */
export function loadOrCreateStakeSigner(cluster: ClusterName): Keypair {
  if (cluster !== "devnet") {
    throw new Error("Only devnet generates a stake signer; elsewhere it comes from the backend");
  }

  const path = join(secretsDir(), "stake-signer-devnet.json");

  try {
    return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(path, "utf8"))));
  } catch {
    const generated = Keypair.generate();
    mkdirSync(secretsDir(), { recursive: true });
    writeFileSync(path, `${JSON.stringify(Array.from(generated.secretKey))}\n`, { mode: 0o600 });
    console.log(`generated a devnet stake signer at ${path}`);

    return generated;
  }
}

const seasonId = (id: bigint): Buffer => {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(id);

  return buf;
};

export function configPda(): PublicKey {
  return PublicKey.findProgramAddressSync([CONFIG_SEED], PROGRAM)[0];
}

export function seasonPda(id: bigint): PublicKey {
  return PublicKey.findProgramAddressSync([SEASON_SEED, seasonId(id)], PROGRAM)[0];
}

/** Authority over the reward vault only — never over principal. */
export function rewardsAuthorityPda(id: bigint): PublicKey {
  return PublicKey.findProgramAddressSync([REWARDS_SEED, seasonId(id)], PROGRAM)[0];
}

export function positionPda(id: bigint, user: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync(
    [POSITION_SEED, seasonId(id), user.toBuffer()],
    PROGRAM,
  )[0];
}

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

export interface ConfigAccount {
  admin: PublicKey;
  pendingAdmin: PublicKey | null;
  stakeSigner: PublicKey;
  mint: PublicKey;
  paused: boolean;
  nextSeason: bigint;
  bump: number;
}

/** Field order must match `Config` in state.rs. */
export function readConfig(data: Buffer): ConfigAccount {
  const reader = new Reader(data);

  return {
    admin: reader.pubkey(),
    pendingAdmin: reader.optionPubkey(),
    stakeSigner: reader.pubkey(),
    mint: reader.pubkey(),
    paused: reader.bool(),
    nextSeason: reader.u64(),
    bump: reader.u8(),
  };
}

export interface SeasonAccount {
  id: bigint;
  startTs: bigint;
  endTs: bigint;
  rewardPool: bigint;
  maxTotalStaked: bigint;
  maxPerWallet: bigint;
  maxAprBps: number;
  maxMultiplierBps: number;
  sweepDelaySeconds: number;
  totalStaked: bigint;
  totalWeighted: bigint;
  weightedStakeSeconds: bigint;
  lastUpdateTs: bigint;
  rewardsPaid: bigint;
  swept: boolean;
  bump: number;
  rewardsBump: number;
}

/** Field order must match `Season` in state.rs, params inlined. */
export function readSeason(data: Buffer): SeasonAccount {
  const reader = new Reader(data);

  return {
    id: reader.u64(),
    startTs: reader.i64(),
    endTs: reader.i64(),
    rewardPool: reader.u64(),
    maxTotalStaked: reader.u64(),
    maxPerWallet: reader.u64(),
    maxAprBps: reader.u16(),
    maxMultiplierBps: reader.u32(),
    sweepDelaySeconds: reader.u32(),
    totalStaked: reader.u64(),
    totalWeighted: reader.u128(),
    weightedStakeSeconds: reader.u128(),
    lastUpdateTs: reader.i64(),
    rewardsPaid: reader.u64(),
    swept: reader.bool(),
    bump: reader.u8(),
    rewardsBump: reader.u8(),
  };
}

export interface PositionAccount {
  amount: bigint;
  multiplierBps: number;
  weightedStakeSeconds: bigint;
  rawStakeSeconds: bigint;
  lastUpdateTs: bigint;
  claimed: boolean;
  bump: number;
}

/** Field order must match `Position` in state.rs. */
export function readPosition(data: Buffer): PositionAccount {
  const reader = new Reader(data);

  return {
    amount: reader.u64(),
    multiplierBps: reader.u32(),
    weightedStakeSeconds: reader.u128(),
    rawStakeSeconds: reader.u128(),
    lastUpdateTs: reader.i64(),
    claimed: reader.bool(),
    bump: reader.u8(),
  };
}

function secretsDir(): string {
  return join(import.meta.dirname, "..", "..", ".secrets");
}

function deploymentsDir(): string {
  return join(import.meta.dirname, "..", "..", "deployments", "stake");
}

export function deploymentPath(cluster: ClusterName): string {
  return join(deploymentsDir(), `${cluster}.json`);
}

export function readDeployment(cluster: ClusterName): Deployment {
  try {
    return JSON.parse(readFileSync(deploymentPath(cluster), "utf8")) as Deployment;
  } catch {
    throw new Error(`No deployment on ${cluster} — run \`pnpm init:${cluster}\` first.`);
  }
}

export function writeDeployment(cluster: ClusterName, deployment: Deployment): void {
  mkdirSync(deploymentsDir(), { recursive: true });
  writeFileSync(deploymentPath(cluster), `${JSON.stringify(deployment, null, 2)}\n`);
}

/** `AdminOnly`: the admin signs, the config is written. */
export function adminInstruction(admin: PublicKey, data: Buffer): TransactionInstruction {
  return new TransactionInstruction({
    programId: PROGRAM,
    keys: [
      { pubkey: admin, isSigner: true, isWritable: false },
      { pubkey: configPda(), isSigner: false, isWritable: true },
    ],
    data,
  });
}

export async function send(
  connection: Connection,
  signers: Keypair[],
  instructions: TransactionInstruction[],
): Promise<string> {
  const signature = await connection.sendTransaction(
    new Transaction().add(...instructions),
    signers,
  );
  await connection.confirmTransaction(signature, "confirmed");
  console.log(`  ${signature}`);

  return signature;
}

/** Seconds since the epoch as an ISO string, for anything printed to a human. */
export function iso(seconds: bigint | number): string {
  return new Date(Number(seconds) * 1000).toISOString();
}

/**
 * The most a season can pay however large its pool is. Mirrors `maxLiability` in
 * `src/lib/stake-math.ts`; printed at `open_season` so an over-funded season is
 * visible before it is signed rather than after it closes.
 */
export function maxLiability(
  maxTotalStaked: bigint,
  maxAprBps: number,
  durationSeconds: bigint,
): bigint {
  return (maxTotalStaked * BigInt(maxAprBps) * durationSeconds) / (10_000n * 31_536_000n);
}
