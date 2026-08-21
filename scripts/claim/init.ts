/**
 * Initializes both programs on a cluster and writes the deployment artifact.
 *
 *   pnpm claim:init:devnet
 *
 * Deploy the bytecode first — `anchor deploy --provider.cluster devnet` — this
 * only creates the config PDAs and vaults. Kept separate because the two have
 * different idempotency: `anchor deploy` re-runs as an upgrade, `initialize`
 * cannot run twice.
 *
 * The airdrop starts with a zero root, so nothing is claimable until `set_root`.
 */

import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import {
  Connection,
  PublicKey,
  SystemProgram,
  Transaction,
  TransactionInstruction,
} from "@solana/web3.js";

import {
  type Deployment,
  PROGRAMS,
  assertWritable,
  configPda,
  deploymentPath,
  loadDeployer,
  loadOrCreateClaimSigner,
  parseCluster,
  writeDeployment,
} from "./common.js";
import { CLUSTERS, DEVNET_WINDOW_SECONDS } from "./config.js";
import { IDLS, discriminator, i64 } from "./encode.js";

const ZERO_ROOT = Buffer.alloc(32);

const cluster = parseCluster();
assertWritable(cluster);

const { rpcUrl, mint: mintAddress } = CLUSTERS[cluster];
const mint = new PublicKey(mintAddress);
const connection = new Connection(rpcUrl, "confirmed");
const admin = loadDeployer();
const claimSigner = loadOrCreateClaimSigner(cluster);

const airdropConfig = configPda(PROGRAMS.airdrop);
const claimConfig = configPda(PROGRAMS.claim);
const airdropVault = getAssociatedTokenAddressSync(mint, airdropConfig, true);
const claimVault = getAssociatedTokenAddressSync(mint, claimConfig, true);

for (const [name, program] of Object.entries(PROGRAMS)) {
  if (!(await connection.getAccountInfo(program))) {
    throw new Error(
      `${name} program ${program.toBase58()} is not deployed on ${cluster}. Run \`anchor deploy --provider.cluster ${cluster}\` first.`,
    );
  }
}

for (const [name, config] of [
  ["airdrop", airdropConfig],
  ["claim", claimConfig],
] as const) {
  if (await connection.getAccountInfo(config)) {
    throw new Error(
      `${name} is already initialized on ${cluster} (config ${config.toBase58()}). Delete nothing — use the admin instructions to change it.`,
    );
  }
}

const now = Math.floor(Date.now() / 1000);
const startTs = now;
const endTs = now + DEVNET_WINDOW_SECONDS;

const airdropInit = new TransactionInstruction({
  programId: PROGRAMS.airdrop,
  keys: [
    { pubkey: admin.publicKey, isSigner: true, isWritable: true },
    { pubkey: airdropConfig, isSigner: false, isWritable: true },
    { pubkey: mint, isSigner: false, isWritable: false },
    { pubkey: airdropVault, isSigner: false, isWritable: true },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: ASSOCIATED_TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([
    discriminator(IDLS.airdrop, "initialize"),
    ZERO_ROOT,
    i64(BigInt(startTs)),
    i64(BigInt(endTs)),
  ]),
});

const claimInit = new TransactionInstruction({
  programId: PROGRAMS.claim,
  keys: [
    { pubkey: admin.publicKey, isSigner: true, isWritable: true },
    { pubkey: claimConfig, isSigner: false, isWritable: true },
    { pubkey: mint, isSigner: false, isWritable: false },
    { pubkey: claimVault, isSigner: false, isWritable: true },
    { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: ASSOCIATED_TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
  ],
  data: Buffer.concat([
    discriminator(IDLS.claim, "initialize"),
    claimSigner.publicKey.toBuffer(),
  ]),
});

const transactions: Record<string, string> = {};

for (const [name, instruction] of [
  ["initializeAirdrop", airdropInit],
  ["initializeClaim", claimInit],
] as const) {
  const signature = await connection.sendTransaction(
    new Transaction().add(instruction),
    [admin],
  );
  await connection.confirmTransaction(signature, "confirmed");
  transactions[name] = signature;
  console.log(`${name}  ${signature}`);
}

const deployment: Deployment = {
  cluster,
  mint: mint.toBase58(),
  admin: admin.publicKey.toBase58(),
  deployedAt: new Date().toISOString(),
  airdrop: {
    programId: PROGRAMS.airdrop.toBase58(),
    config: airdropConfig.toBase58(),
    vault: airdropVault.toBase58(),
    root: ZERO_ROOT.toString("hex"),
    startTs,
    endTs,
  },
  claim: {
    programId: PROGRAMS.claim.toBase58(),
    config: claimConfig.toBase58(),
    vault: claimVault.toBase58(),
    claimSigner: claimSigner.publicKey.toBase58(),
  },
  transactions,
};

writeDeployment(cluster, deployment);

console.log(`\nwrote ${deploymentPath(cluster)}`);
console.log("airdrop is unpaused with a zero root — nothing claimable until set_root");
