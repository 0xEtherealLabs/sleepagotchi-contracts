/**
 * Reads the on-chain state of both programs. Read-only unless `--claim` is passed.
 *
 *   pnpm claim:harness:devnet                # dump state, sign nothing
 *   pnpm claim:harness:devnet -- --claim     # full rehearsal, see below
 *
 * `--claim` publishes a one-wallet root for the deployer, funds the airdrop vault
 * from the deployer's own tokens, and claims it. It is a real end-to-end exercise
 * of the path a user takes, which local LiteSVM tests cannot prove: a live cluster
 * has real rent, real ATA creation and the real token program.
 *
 * It permanently spends the deployer's one airdrop claim on this cluster — the
 * receipt PDA is seeded by wallet alone and never expires.
 */

import {
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createTransferInstruction,
  getAccount,
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
  parseCluster,
  assertWritable,
  loadDeployer,
  PROGRAMS,
  readAirdropConfig,
  readClaimConfig,
  readDeployment,
  receiptPda,
} from "./common.js";
import { CLUSTERS } from "./config.js";
import { IDLS, Reader, discriminator, u64, vecOf32 } from "./encode.js";
import { build, hex, proof } from "./tree.js";

const cluster = parseCluster();
const write = process.argv.includes("--claim");

const { rpcUrl } = CLUSTERS[cluster];
const connection = new Connection(rpcUrl, "confirmed");
const deployment = readDeployment(cluster);
const wallet = loadDeployer();
const mint = new PublicKey(deployment.mint);

async function dump(): Promise<void> {
  const airdropConfig = await connection.getAccountInfo(new PublicKey(deployment.airdrop.config));
  const claimConfig = await connection.getAccountInfo(new PublicKey(deployment.claim.config));

  if (!airdropConfig || !claimConfig) {
    throw new Error("a config PDA is missing — was init run against this cluster?");
  }

  const airdrop = readAirdropConfig(airdropConfig.data);
  const claim = readClaimConfig(claimConfig.data);

  const now = Math.floor(Date.now() / 1000);
  const windowState =
    now < airdrop.startTs ? "not started" : now < airdrop.endTs ? "OPEN" : "ended";

  console.log(`cluster        ${cluster}`);
  console.log(`mint           ${deployment.mint}`);
  console.log();
  console.log("airdrop");
  console.log(`  admin        ${airdrop.admin}`);
  console.log(`  root         ${airdrop.root}${isZero(airdrop.root) ? "  (unset)" : ""}`);
  console.log(`  window       ${iso(airdrop.startTs)} -> ${iso(airdrop.endTs)}  [${windowState}]`);
  console.log(`  paused       ${airdrop.paused}`);
  console.log(`  vault        ${await balance(deployment.airdrop.vault)}`);
  console.log();
  console.log("claim");
  console.log(`  admin        ${claim.admin}`);
  console.log(`  claimSigner  ${claim.claimSigner}`);
  console.log(`  paused       ${claim.paused}`);
  console.log(`  vault        ${await balance(deployment.claim.vault)}`);

  const receipt = await connection.getAccountInfo(receiptPda(PROGRAMS.airdrop, wallet.publicKey));
  console.log();
  console.log(`this wallet    ${wallet.publicKey.toBase58()}`);
  console.log(`  airdrop receipt  ${receipt ? "present — already claimed" : "none"}`);
}

async function rehearse(): Promise<void> {
  assertWritable(cluster);

  const allocation = 1_000_000_000n;
  const tree = build([{ user: wallet.publicKey.toBase58(), amount: allocation }]);
  const airdropConfig = new PublicKey(deployment.airdrop.config);
  const vault = new PublicKey(deployment.airdrop.vault);
  const destination = getAssociatedTokenAddressSync(mint, wallet.publicKey);

  if (await connection.getAccountInfo(receiptPda(PROGRAMS.airdrop, wallet.publicKey))) {
    throw new Error(
      `${wallet.publicKey.toBase58()} has already claimed on ${cluster}. One claim per wallet, permanently — rehearse with a different keypair via SOLANA_DEPLOYER_KEYPAIR.`,
    );
  }

  console.log(`publishing a one-wallet root ${hex(tree.root)}`);
  await send([
    new TransactionInstruction({
      programId: PROGRAMS.airdrop,
      keys: [
        { pubkey: wallet.publicKey, isSigner: true, isWritable: false },
        { pubkey: airdropConfig, isSigner: false, isWritable: true },
      ],
      data: Buffer.concat([discriminator(IDLS.airdrop, "set_root"), Buffer.from(tree.root)]),
    }),
  ]);

  const source = getAssociatedTokenAddressSync(mint, wallet.publicKey);
  const held = await getAccount(connection, source);
  if (held.amount < allocation) {
    throw new Error(`wallet holds ${held.amount} base units, needs ${allocation} to fund the vault`);
  }

  console.log("funding the airdrop vault");
  await send([
    createAssociatedTokenAccountIdempotentInstruction(
      wallet.publicKey,
      vault,
      airdropConfig,
      mint,
    ),
    createTransferInstruction(source, vault, wallet.publicKey, allocation),
  ]);

  console.log("claiming");
  const before = (await getAccount(connection, destination)).amount;
  await send([
    createAssociatedTokenAccountIdempotentInstruction(
      wallet.publicKey,
      destination,
      wallet.publicKey,
      mint,
    ),
    new TransactionInstruction({
      programId: PROGRAMS.airdrop,
      keys: [
        { pubkey: wallet.publicKey, isSigner: true, isWritable: true },
        { pubkey: airdropConfig, isSigner: false, isWritable: false },
        { pubkey: receiptPda(PROGRAMS.airdrop, wallet.publicKey), isSigner: false, isWritable: true },
        { pubkey: mint, isSigner: false, isWritable: false },
        { pubkey: vault, isSigner: false, isWritable: true },
        { pubkey: destination, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      data: Buffer.concat([
        discriminator(IDLS.airdrop, "claim"),
        u64(allocation),
        vecOf32(proof(tree, 0).map((h) => Buffer.from(h))),
      ]),
    }),
  ]);

  const after = (await getAccount(connection, destination)).amount;
  console.log(`\nreceived ${after - before} base units (expected ${allocation})`);
  if (after - before !== allocation) {
    throw new Error("claim did not pay the allocation");
  }
}

async function send(instructions: TransactionInstruction[]): Promise<void> {
  const signature = await connection.sendTransaction(
    new Transaction().add(...instructions),
    [wallet],
  );
  await connection.confirmTransaction(signature, "confirmed");
  console.log(`  ${signature}`);
}

async function balance(address: string): Promise<string> {
  try {
    return `${(await getAccount(connection, new PublicKey(address))).amount} base units`;
  } catch {
    return "no account";
  }
}

function iso(seconds: number): string {
  return new Date(seconds * 1000).toISOString();
}

function isZero(hexString: string): boolean {
  return /^0+$/.test(hexString);
}

await dump();

if (write) {
  console.log("\n--- rehearsal (writes) ---");
  await rehearse();
} else {
  console.log("\nread-only. pass --claim to rehearse a real claim.");
}
