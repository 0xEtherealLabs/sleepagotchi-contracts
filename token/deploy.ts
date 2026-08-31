/**
 * Deploys the SPL mint. Full supply minted once, then the mint authority is
 * revoked — there is no program, and no way to issue more.
 *
 *   pnpm token:deploy:devnet
 *   pnpm token:deploy:mainnet   # refuses, see CLUSTERS in config.mts
 */

import { existsSync } from "node:fs";

import {
  createMetadataAccountV3,
  findMetadataPda,
  mplTokenMetadata,
} from "@metaplex-foundation/mpl-token-metadata";
import { keypairIdentity, publicKey } from "@metaplex-foundation/umi";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";
import {
  AuthorityType,
  createMint,
  getMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  setAuthority,
} from "@solana/spl-token";
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
} from "@solana/web3.js";
import bs58 from "bs58";

import {
  artifactPath,
  loadDeployer,
  loadMintKeypair,
  parseCluster,
  writeArtifact,
} from "./common";
import {
  CLUSTERS,
  METADATA_IS_MUTABLE,
  TOKEN_DECIMALS,
  TOTAL_SUPPLY,
  TOTAL_SUPPLY_BASE_UNITS,
  type ClusterConfig,
} from "./config";

const attachMetadata = async (
  connection: Connection,
  deployer: Keypair,
  treasury: PublicKey,
  mint: PublicKey,
  config: ClusterConfig,
) => {
  // Share the `confirmed` connection. Umi defaults to `finalized`, which would
  // simulate against a bank predating the mint we just created — the program
  // then panics reading an empty account.
  const umi = createUmi(connection).use(mplTokenMetadata());
  umi.use(
    keypairIdentity(umi.eddsa.createKeypairFromSecretKey(deployer.secretKey)),
  );

  const mintKey = publicKey(mint.toBase58());
  const { signature } = await createMetadataAccountV3(umi, {
    mint: mintKey,
    mintAuthority: umi.identity,
    payer: umi.identity,
    updateAuthority: publicKey(treasury.toBase58()),
    data: {
      name: config.name,
      symbol: config.symbol,
      uri: config.metadataUri,
      sellerFeeBasisPoints: 0,
      creators: null,
      collection: null,
      uses: null,
    },
    isMutable: METADATA_IS_MUTABLE,
    collectionDetails: null,
  }).sendAndConfirm(umi, {
    send: { preflightCommitment: "confirmed" },
    confirm: { commitment: "confirmed" },
  });

  return {
    metadataPda: new PublicKey(findMetadataPda(umi, { mint: mintKey })[0]),
    signature: bs58.encode(signature),
  };
};

const main = async () => {
  const cluster = parseCluster();
  const config = CLUSTERS[cluster];

  if (config.blockedReason) {
    throw new Error(`Refusing to deploy to ${cluster}.\n\n${config.blockedReason}`);
  }
  if (existsSync(artifactPath(cluster)) && !process.argv.includes("--force")) {
    throw new Error(
      `${artifactPath(cluster)} already exists.\nA mint cannot be redeployed in place — re-running creates a second, unrelated token. Pass --force if that is what you want.`,
    );
  }

  const deployer = loadDeployer();
  // Read before the first transaction: a bad path should fail here, not after
  // the mint exists at some other address.
  const mintKeypair = loadMintKeypair();
  const treasury = config.treasury
    ? new PublicKey(config.treasury)
    : deployer.publicKey;
  const connection = new Connection(
    process.env.SOLANA_RPC_URL ?? config.rpcUrl,
    "confirmed",
  );

  const sol = (await connection.getBalance(deployer.publicKey)) / LAMPORTS_PER_SOL;
  if (sol < 0.05) {
    throw new Error(
      `Deployer has ${sol} SOL. Fund ${deployer.publicKey.toBase58()} at https://faucet.solana.com and re-run.`,
    );
  }

  console.log(`Deploying ${config.symbol} (${config.name}) to ${cluster}`);
  console.log(`  deployer: ${deployer.publicKey.toBase58()}`);
  console.log(
    `  treasury: ${treasury.toBase58()}${config.treasury ? "" : " (deployer)"}`,
  );
  if (mintKeypair) {
    console.log(`  mint key: ${mintKeypair.publicKey.toBase58()} (pre-generated)`);
  }

  const mint = await createMint(
    connection,
    deployer,
    deployer.publicKey,
    // Freeze authority: never granted, rather than granted and revoked.
    null,
    TOKEN_DECIMALS,
    // Undefined falls through to createMint's own `Keypair.generate()`.
    mintKeypair,
  );
  console.log(`\n  mint:     ${mint.toBase58()}`);

  const metadata = await attachMetadata(
    connection,
    deployer,
    treasury,
    mint,
    config,
  );
  console.log(`  metadata: ${metadata.metadataPda.toBase58()}`);

  const treasuryAta = await getOrCreateAssociatedTokenAccount(
    connection,
    deployer,
    mint,
    treasury,
    // A Squads vault is a PDA, so it is off the ed25519 curve.
    true,
  );
  const mintSupply = await mintTo(
    connection,
    deployer,
    mint,
    treasuryAta.address,
    deployer,
    TOTAL_SUPPLY_BASE_UNITS,
  );
  console.log(`  supply:   ${TOTAL_SUPPLY} ${config.symbol} → ${treasuryAta.address.toBase58()}`);

  const revokeMintAuthority = await setAuthority(
    connection,
    deployer,
    mint,
    deployer,
    AuthorityType.MintTokens,
    null,
  );

  // Every step above is irreversible, so trust the chain rather than the script.
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

  writeArtifact(cluster, {
    cluster,
    mint: mint.toBase58(),
    name: config.name,
    symbol: config.symbol,
    decimals: TOKEN_DECIMALS,
    // A string — JSON has no bigint and this must not become a float.
    totalSupplyBaseUnits: TOTAL_SUPPLY_BASE_UNITS.toString(),
    metadataPda: metadata.metadataPda.toBase58(),
    metadataUri: config.metadataUri,
    authorities: {
      mint: null,
      freeze: null,
      metadataUpdate: treasury.toBase58(),
    },
    treasury: {
      owner: treasury.toBase58(),
      tokenAccount: treasuryAta.address.toBase58(),
    },
    deployer: deployer.publicKey.toBase58(),
    deployedAt: new Date().toISOString(),
    transactions: {
      metadata: metadata.signature,
      mintSupply,
      revokeMintAuthority,
    },
  });

  console.log(
    `\nVerified: mint and freeze authorities revoked.\nhttps://explorer.solana.com/address/${mint.toBase58()}?cluster=${cluster}`,
  );
};

main().catch((error: unknown) => {
  console.error(`\n${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
