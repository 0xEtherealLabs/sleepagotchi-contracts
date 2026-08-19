/**
 * Sends tokens from the treasury to a wallet — a dev wallet on devnet, the
 * thirdweb backend wallet in production.
 *
 *   pnpm token:transfer:devnet <recipient> <amount>
 *
 * Amount is in whole tokens, e.g. `1000` or `12.5`.
 */

import {
  getAccount,
  getAssociatedTokenAddress,
  getOrCreateAssociatedTokenAccount,
  transferChecked,
} from "@solana/spl-token";
import { Connection, PublicKey } from "@solana/web3.js";

import {
  loadDeployer,
  parseCluster,
  readArtifact,
  toBaseUnits,
  toDisplay,
} from "./common";
import { CLUSTERS } from "./config";

const main = async () => {
  const cluster = parseCluster();
  const config = CLUSTERS[cluster];
  const [recipientArg, amountArg] = process.argv.slice(3);

  if (!recipientArg || !amountArg) {
    throw new Error(
      `Usage: pnpm token:transfer:${cluster === "mainnet-beta" ? "mainnet" : cluster} <recipient> <amount>`,
    );
  }

  // The supply lives with the treasury. Where that is a Squads multisig, a
  // local keypair cannot sign for it — those transfers go through Squads.
  if (config.treasury) {
    throw new Error(
      `The ${cluster} treasury is a multisig (${config.treasury}). Propose the transfer in Squads instead.`,
    );
  }

  const artifact = readArtifact(cluster);
  const mint = new PublicKey(artifact.mint);

  let recipient: PublicKey;
  try {
    recipient = new PublicKey(recipientArg);
  } catch {
    throw new Error(`"${recipientArg}" is not a valid Solana address.`);
  }

  const amount = toBaseUnits(amountArg, artifact.decimals);
  if (amount === 0n) throw new Error("Amount must be greater than zero.");

  const deployer = loadDeployer();
  const connection = new Connection(config.rpcUrl, "confirmed");

  const source = await getAssociatedTokenAddress(mint, deployer.publicKey);
  const balance = (await getAccount(connection, source)).amount;
  if (balance < amount) {
    throw new Error(
      `Treasury holds ${toDisplay(balance, artifact.decimals)} ${artifact.symbol}, cannot send ${amountArg}.`,
    );
  }

  console.log(`Sending ${amountArg} ${artifact.symbol} on ${cluster}`);
  console.log(`  from: ${deployer.publicKey.toBase58()}`);
  console.log(`  to:   ${recipient.toBase58()}`);

  const destination = await getOrCreateAssociatedTokenAccount(
    connection,
    deployer,
    mint,
    recipient,
    true,
  );

  const signature = await transferChecked(
    connection,
    deployer,
    source,
    mint,
    destination.address,
    deployer,
    amount,
    artifact.decimals,
  );

  const remaining = (await getAccount(connection, source)).amount;
  console.log(
    `\nSent. Treasury now holds ${toDisplay(remaining, artifact.decimals)} ${artifact.symbol}.`,
  );
  console.log(`https://explorer.solana.com/tx/${signature}?cluster=${cluster}`);
};

main().catch((error: unknown) => {
  console.error(`\n${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
