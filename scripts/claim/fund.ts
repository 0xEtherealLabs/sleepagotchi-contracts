/**
 * Funds the airdrop vault with exactly what the tree owes.
 *
 *   pnpm claim:fund:devnet
 *   pnpm claim:fund:devnet -- --dry-run
 *
 * The total is knowable — it is the sum of every allocation — so the vault gets
 * that and not a round number. An underfunded vault fails the last claimants
 * with `InsufficientVault` after everyone before them succeeded, which is the
 * worst possible order to find out in.
 *
 * Tops up rather than transfers blindly: it moves the difference between what
 * the vault holds and what it needs, so running it twice is safe and running it
 * after a partial transfer finishes the job.
 */

import {
  createAssociatedTokenAccountIdempotentInstruction,
  createTransferInstruction,
  getAccount,
  getAssociatedTokenAddressSync,
  getMint,
} from "@solana/spl-token";
import { Connection, PublicKey } from "@solana/web3.js";

import {
  assertWritable,
  loadDeployer,
  parseCluster,
  readDeployment,
  readPublished,
  send,
} from "./common.js";
import { CLUSTERS } from "./config.js";

const cluster = parseCluster();
const dryRun = process.argv.includes("--dry-run");

if (!dryRun) assertWritable(cluster);

const connection = new Connection(CLUSTERS[cluster].rpcUrl, "confirmed");
const deployment = readDeployment(cluster);
const wallet = loadDeployer();
const published = readPublished();

const mint = new PublicKey(deployment.mint);
const config = new PublicKey(deployment.airdrop.config);
const vault = new PublicKey(deployment.airdrop.vault);
const source = getAssociatedTokenAddressSync(mint, wallet.publicKey);

const needed = BigInt(published.totalBaseUnits);

const balance = async (address: PublicKey): Promise<bigint> => {
  try {
    return (await getAccount(connection, address)).amount;
  } catch {
    // No account yet, which for the vault is the ordinary case before the
    // first funding and for the source means the deployer holds none.
    return 0n;
  }
};

const { decimals } = await getMint(connection, mint);
const tokens = (base: bigint) => {
  const scale = 10n ** BigInt(decimals);
  const whole = (base / scale).toLocaleString("en-US");

  return `${whole}.${(base % scale).toString().padStart(decimals, "0").slice(0, 4)}`;
};

const held = await balance(vault);
const wallet_ = await balance(source);

console.log(`cluster    ${cluster}`);
console.log(`mint       ${mint.toBase58()}`);
console.log(`vault      ${vault.toBase58()}`);
console.log(`claimants  ${published.claimants}`);
console.log();
console.log(`needed     ${needed} (${tokens(needed)})`);
console.log(`in vault   ${held} (${tokens(held)})`);
console.log(`in wallet  ${wallet_} (${tokens(wallet_)})`);

if (held >= needed) {
  console.log(
    held === needed
      ? "\nvault is funded exactly — nothing to do."
      : `\nvault already holds ${held - needed} base units more than the tree owes. Nothing to do; withdraw the excess with the admin instruction if it should not be there.`,
  );
  process.exit(0);
}

const transfer = needed - held;
console.log(`transfer   ${transfer} (${tokens(transfer)})`);

if (wallet_ < transfer) {
  throw new Error(
    `the deployer holds ${wallet_} base units and needs ${transfer}. Short by ${transfer - wallet_}.`,
  );
}

if (dryRun) {
  console.log("\ndry run — nothing sent.");
  process.exit(0);
}

console.log("\nfunding");
await send(connection, wallet, [
  // Idempotent: the vault ATA is created by `initialize`, but a cluster where
  // that was rolled back should not need a different command.
  createAssociatedTokenAccountIdempotentInstruction(
    wallet.publicKey,
    vault,
    config,
    mint,
  ),
  createTransferInstruction(source, vault, wallet.publicKey, transfer),
]);

const after = await balance(vault);
console.log(`\nvault now  ${after} (${tokens(after)})`);

if (after !== needed) {
  throw new Error(
    `vault holds ${after} base units, expected ${needed}. Re-run to top up.`,
  );
}

console.log("funded exactly.");
