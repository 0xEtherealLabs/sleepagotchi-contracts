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
import { Reader } from "./encode.js";

export const PROGRAMS = {
  airdrop: new PublicKey("DM6GCMdgn9daiiUCCaiCjrQHWwe3RiNJK3EPcqa2sdBg"),
  claim: new PublicKey("8igHauFjDNKpTkyATw9cigy7XWxWMbvHotA4S9i4qe1f"),
} as const;

export const CONFIG_SEED = Buffer.from("config");
export const RECEIPT_SEED = Buffer.from("receipt");

export interface AirdropConfigAccount {
  admin: PublicKey;
  pendingAdmin: PublicKey | null;
  root: string;
  mint: PublicKey;
  startTs: number;
  endTs: number;
  paused: boolean;
  bump: number;
}

export interface ClaimConfigAccount {
  admin: PublicKey;
  pendingAdmin: PublicKey | null;
  claimSigner: PublicKey;
  mint: PublicKey;
  paused: boolean;
  bump: number;
}

/**
 * Field order must match `Config` in the airdrop's `state.rs`.
 *
 * One decoder rather than a `Reader` walked by hand at each call site: this is a
 * second copy of the program's layout, and `pnpm check` can only hold one copy
 * honest. `pending_admin` is an `Option`, so it is one byte when unset and
 * thirty-three when set — reading it at a fixed width shifts everything after.
 */
export function readAirdropConfig(data: Buffer): AirdropConfigAccount {
  const reader = new Reader(data);

  return {
    admin: reader.pubkey(),
    pendingAdmin: reader.optionPubkey(),
    root: reader.bytes32().toString("hex"),
    mint: reader.pubkey(),
    startTs: Number(reader.i64()),
    endTs: Number(reader.i64()),
    paused: reader.bool(),
    bump: reader.u8(),
  };
}

/** Field order must match `Config` in the claim program's `state.rs`. */
export function readClaimConfig(data: Buffer): ClaimConfigAccount {
  const reader = new Reader(data);

  return {
    admin: reader.pubkey(),
    pendingAdmin: reader.optionPubkey(),
    claimSigner: reader.pubkey(),
    mint: reader.pubkey(),
    paused: reader.bool(),
    bump: reader.u8(),
  };
}

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
 * Same convention as `contracts/token/common.ts` — a path to a Solana CLI keypair, which is
 * a JSON array of the 64 secret key bytes. The only thing read from the env.
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
 * The devnet claim signer. Generated on first use and kept out of git — it stands
 * in for whatever the backend holds via `CLAIM_SIGNER_SECRET_KEY`, and is never
 * funded, so losing it costs a `set_claim_signer` and nothing else.
 */
export function loadOrCreateClaimSigner(cluster: ClusterName): Keypair {
  if (cluster !== "devnet") {
    throw new Error("Only devnet generates a claim signer; elsewhere it comes from the backend");
  }

  const path = join(secretsDir(), "claim-signer-devnet.json");

  try {
    return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(path, "utf8"))));
  } catch {
    const generated = Keypair.generate();
    mkdirSync(secretsDir(), { recursive: true });
    writeFileSync(path, `${JSON.stringify(Array.from(generated.secretKey))}\n`, { mode: 0o600 });
    console.log(`generated a devnet claim signer at ${path}`);

    return generated;
  }
}

export function configPda(program: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([CONFIG_SEED], program)[0];
}

export function receiptPda(program: PublicKey, user: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([RECEIPT_SEED, user.toBuffer()], program)[0];
}

export interface Deployment {
  cluster: ClusterName;
  mint: string;
  admin: string;
  deployedAt: string;
  airdrop: {
    programId: string;
    config: string;
    vault: string;
    root: string;
    startTs: number;
    endTs: number;
  };
  claim: {
    programId: string;
    config: string;
    vault: string;
    /** Public half only. The secret lives in `CLAIM_SIGNER_SECRET_KEY`. */
    claimSigner: string;
  };
  transactions: Record<string, string>;
}

function secretsDir(): string {
  return join(import.meta.dirname, "..", "..", ".secrets");
}

function deploymentsDir(): string {
  return join(import.meta.dirname, "..", "..", "deployments", "claim");
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

/**
 * The root and total the app is shipping, written by `pnpm airdrop:snapshot`.
 *
 * Read from the app rather than passed on the command line. These two numbers
 * and the allocation list they came from have to agree, and a root typed by hand
 * is a root that can be typed wrong — the transaction would succeed, and every
 * proof the app serves would fail on chain afterwards.
 */
export interface Published {
  root: string;
  claimants: number;
  totalBaseUnits: string;
}

export function readPublished(): Published {
  const path = join(
    import.meta.dirname,
    "..",
    "..",
    "..",
    "src",
    "config",
    "airdrop",
    "root.json",
  );

  let published: Published;
  try {
    published = JSON.parse(readFileSync(path, "utf8")) as Published;
  } catch {
    throw new Error(`No snapshot at ${path} — run \`pnpm airdrop:snapshot\` first.`);
  }

  if (!/^[0-9a-f]{64}$/i.test(published.root)) {
    throw new Error(`${path} has no usable root.`);
  }
  if (/^0+$/.test(published.root)) {
    throw new Error(`${path} holds the zero root, which claims nothing.`);
  }

  return published;
}

/** `AdminOnly`: the admin signs, the config is written. */
export function adminInstruction(
  program: PublicKey,
  admin: PublicKey,
  data: Buffer,
): TransactionInstruction {
  return new TransactionInstruction({
    programId: program,
    keys: [
      { pubkey: admin, isSigner: true, isWritable: false },
      { pubkey: configPda(program), isSigner: false, isWritable: true },
    ],
    data,
  });
}

export async function send(
  connection: Connection,
  signer: Keypair,
  instructions: TransactionInstruction[],
): Promise<string> {
  const signature = await connection.sendTransaction(
    new Transaction().add(...instructions),
    [signer],
  );
  await connection.confirmTransaction(signature, "confirmed");
  console.log(`  ${signature}`);

  return signature;
}

/** Seconds since the epoch as an ISO string, for anything printed to a human. */
export function iso(seconds: number): string {
  return new Date(seconds * 1000).toISOString();
}
