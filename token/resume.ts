/**
 * Finishes a deploy that died partway.
 *
 *   pnpm token:resume:mainnet -- --dry-run
 *   pnpm token:resume:mainnet
 *
 * `deploy.ts` is five irreversible steps in a row — create the mint, attach
 * metadata, open the treasury account, mint the supply, revoke the authority.
 * A transient RPC failure between any two of them leaves a real mint on chain
 * with no artifact and no way to re-run: `createMint` would refuse, because the
 * address is already taken by the half-finished attempt.
 *
 * This picks up from wherever that stopped. Every step is guarded by what the
 * chain actually says rather than by what the last run printed, so it is safe to
 * run twice — the second run finds nothing left to do and says so.
 *
 * It does NOT create the mint. If none exists yet, that is a fresh `deploy.ts`.
 */

import { existsSync } from "node:fs";

import { findMetadataPda, mplTokenMetadata } from "@metaplex-foundation/mpl-token-metadata";
import { publicKey } from "@metaplex-foundation/umi";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";
import {
  AuthorityType,
  getMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  setAuthority,
} from "@solana/spl-token";
import { Connection, PublicKey } from "@solana/web3.js";

import {
  artifactPath,
  loadDeployer,
  loadMintKeypair,
  parseCluster,
  writeArtifact,
} from "./common";
import {
  CLUSTERS,
  TOKEN_DECIMALS,
  TOTAL_SUPPLY,
  TOTAL_SUPPLY_BASE_UNITS,
} from "./config";

const flag = (name: string) => {
  const i = process.argv.indexOf(`--${name}`);
  return i === -1 ? undefined : process.argv[i + 1];
};

const main = async () => {
  const cluster = parseCluster();
  const config = CLUSTERS[cluster];
  const dryRun = process.argv.includes("--dry-run");

  if (existsSync(artifactPath(cluster)) && !process.argv.includes("--force")) {
    throw new Error(
      `${artifactPath(cluster)} already exists — that deploy finished. Pass --force only if you know the artifact is wrong.`,
    );
  }

  // The mint address, not a signer: `mint_to` and `set_authority` are signed by
  // the mint authority, which is the deployer. `--mint` covers a resume on a
  // machine where the pre-generated keypair is not the one in .env.
  const override = flag("mint");
  const mint = override
    ? new PublicKey(override)
    : loadMintKeypair()?.publicKey;
  if (!mint) {
    throw new Error(
      "No mint address. Set SOLANA_MINT_KEYPAIR, or pass `-- --mint <address>`.",
    );
  }

  const deployer = loadDeployer();
  const treasury = config.treasury
    ? new PublicKey(config.treasury)
    : deployer.publicKey;
  const connection = new Connection(config.rpcUrl, "confirmed");

  const before = await getMint(connection, mint);

  console.log(`Resuming ${config.symbol} on ${cluster}`);
  console.log(`  mint:     ${mint.toBase58()}`);
  console.log(`  deployer: ${deployer.publicKey.toBase58()}`);
  console.log(`  treasury: ${treasury.toBase58()}${config.treasury ? "" : " (deployer)"}`);
  console.log(`  supply:   ${before.supply} / ${TOTAL_SUPPLY_BASE_UNITS}`);
  console.log(`  mint authority: ${before.mintAuthority?.toBase58() ?? "revoked"}`);

  // Anything below is a property `deploy.ts` would have established up front, so
  // a mismatch means this is not the mint that run created.
  if (before.decimals !== TOKEN_DECIMALS) {
    throw new Error(`Mint has ${before.decimals} decimals, expected ${TOKEN_DECIMALS}.`);
  }
  if (before.freezeAuthority) {
    throw new Error(`Mint has a freeze authority (${before.freezeAuthority.toBase58()}); this deploy never grants one.`);
  }
  if (before.supply !== 0n && before.supply !== TOTAL_SUPPLY_BASE_UNITS) {
    throw new Error(`Mint holds ${before.supply} base units, which is neither zero nor the full supply. Resolve by hand.`);
  }
  if (before.supply === 0n && !before.mintAuthority) {
    throw new Error("Supply is zero and the mint authority is revoked — nothing can ever be minted. This mint is dead.");
  }
  if (before.mintAuthority && !before.mintAuthority.equals(deployer.publicKey)) {
    throw new Error(`Mint authority is ${before.mintAuthority.toBase58()}, but the deployer is ${deployer.publicKey.toBase58()}.`);
  }

  const umi = createUmi(connection).use(mplTokenMetadata());
  const metadataPda = new PublicKey(
    findMetadataPda(umi, { mint: publicKey(mint.toBase58()) })[0],
  );
  if (!(await connection.getAccountInfo(metadataPda))) {
    throw new Error(
      `No metadata account at ${metadataPda.toBase58()}. This resumes minting and revocation only — attach metadata first.`,
    );
  }

  if (dryRun) {
    console.log("\nwould:");
    console.log(before.supply === 0n ? `  mint ${TOTAL_SUPPLY} ${config.symbol} to the treasury` : "  (supply already minted)");
    console.log(before.mintAuthority ? "  revoke the mint authority" : "  (mint authority already revoked)");
    console.log(`  write ${artifactPath(cluster)}`);
    return;
  }

  const treasuryAta = await getOrCreateAssociatedTokenAccount(
    connection,
    deployer,
    mint,
    treasury,
    // A Squads vault is a PDA, so it is off the ed25519 curve.
    true,
  );

  const transactions: Record<string, string> = {};

  if (before.supply === 0n) {
    transactions.mintSupply = await mintTo(
      connection,
      deployer,
      mint,
      treasuryAta.address,
      deployer,
      TOTAL_SUPPLY_BASE_UNITS,
    );
    console.log(`\n  minted ${TOTAL_SUPPLY} ${config.symbol} → ${treasuryAta.address.toBase58()}`);
  } else {
    console.log("\n  supply already minted, skipping");
  }

  if (before.mintAuthority) {
    transactions.revokeMintAuthority = await setAuthority(
      connection,
      deployer,
      mint,
      deployer,
      AuthorityType.MintTokens,
      null,
    );
    console.log("  mint authority revoked");
  } else {
    console.log("  mint authority already revoked, skipping");
  }

  // Same assertion `deploy.ts` makes: trust the chain, not this script.
  const state = await getMint(connection, mint);
  if (
    state.mintAuthority ||
    state.freezeAuthority ||
    state.decimals !== TOKEN_DECIMALS ||
    state.supply !== TOTAL_SUPPLY_BASE_UNITS
  ) {
    throw new Error(
      `Unexpected mint state: mint=${state.mintAuthority} freeze=${state.freezeAuthority} decimals=${state.decimals} supply=${state.supply}`,
    );
  }

  // The metadata transaction belongs to the run that died, so it is recovered
  // from the chain rather than invented — it is the oldest signature the
  // metadata account has.
  const signatures = await connection.getSignaturesForAddress(metadataPda, { limit: 1000 });
  const metadataSignature = signatures.at(-1)?.signature ?? "unrecovered";

  writeArtifact(cluster, {
    cluster,
    mint: mint.toBase58(),
    name: config.name,
    symbol: config.symbol,
    decimals: TOKEN_DECIMALS,
    totalSupplyBaseUnits: TOTAL_SUPPLY_BASE_UNITS.toString(),
    metadataPda: metadataPda.toBase58(),
    metadataUri: config.metadataUri,
    authorities: { mint: null, freeze: null, metadataUpdate: treasury.toBase58() },
    treasury: { owner: treasury.toBase58(), tokenAccount: treasuryAta.address.toBase58() },
    deployer: deployer.publicKey.toBase58(),
    deployedAt: new Date().toISOString(),
    transactions: { metadata: metadataSignature, ...transactions },
  });

  console.log(
    `\nVerified: mint and freeze authorities revoked.\nhttps://explorer.solana.com/address/${mint.toBase58()}`,
  );
};

main().catch((error: unknown) => {
  console.error(`\n${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
