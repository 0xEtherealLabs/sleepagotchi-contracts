/**
 * Rewrites the mint's on-chain name, symbol and metadata URI.
 *
 *   pnpm token:rename:devnet
 *
 * This is how a mint's identity changes without a new mint. The mint authority
 * being revoked is irrelevant here — that governs supply. What this needs is the
 * *metadata update* authority, which `deploy.ts` left with the treasury, and a
 * metadata account that was created mutable.
 *
 * Everything it writes is reversible by running it again, which is what makes it
 * safe to point at devnet repeatedly. The one thing to understand before running
 * it on mainnet is that renaming a live token silently changes what every wallet
 * and explorer calls it, and the caches make that neither instant nor uniform.
 */

import {
  fetchMetadata,
  findMetadataPda,
  mplTokenMetadata,
  updateMetadataAccountV2,
} from "@metaplex-foundation/mpl-token-metadata";
import { keypairIdentity, publicKey } from "@metaplex-foundation/umi";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";
import { Connection, PublicKey } from "@solana/web3.js";
import bs58 from "bs58";

import { loadDeployer, parseCluster, readArtifact, writeArtifact } from "./common";
import { CLUSTERS } from "./config";

/** Token Metadata pads its fixed-width strings with NULs. */
const clean = (value: string) => value.replace(/\0+$/, "");

/** The program's fixed widths. Exceeding one truncates rather than failing. */
const LIMITS = { name: 32, symbol: 10, uri: 200 } as const;

const main = async () => {
  const cluster = parseCluster();
  const config = CLUSTERS[cluster];

  if (config.blockedReason) {
    throw new Error(`Refusing to write to ${cluster}.\n\n${config.blockedReason}`);
  }
  if (!config.metadataUri) {
    throw new Error(
      `CLUSTERS["${cluster}"].metadataUri is empty. Run \`pnpm token:upload:${cluster === "devnet" ? "devnet" : "mainnet"}\` and paste the URL it prints into contracts/token/config.ts.`,
    );
  }

  for (const [field, limit] of Object.entries(LIMITS)) {
    const value = field === "uri" ? config.metadataUri : config[field as "name" | "symbol"];
    if (value.length > limit) {
      throw new Error(
        `${field} is ${value.length} characters, over the program's ${limit}. It would be silently truncated on chain.`,
      );
    }
  }

  const artifact = readArtifact(cluster);
  const deployer = loadDeployer();
  const connection = new Connection(config.rpcUrl, "confirmed");

  const umi = createUmi(connection).use(mplTokenMetadata());
  umi.use(keypairIdentity(umi.eddsa.createKeypairFromSecretKey(deployer.secretKey)));

  const mintKey = publicKey(artifact.mint);
  const metadataPda = findMetadataPda(umi, { mint: mintKey });
  const before = await fetchMetadata(umi, metadataPda);

  // Read the chain rather than trusting the artifact: the artifact records what
  // a past run intended, and the authority is the thing that decides whether
  // this run can do anything at all.
  if (!before.isMutable) {
    throw new Error(
      `${artifact.mint} has immutable metadata. Nothing can change its name — this is permanent.`,
    );
  }
  if (before.updateAuthority !== publicKey(deployer.publicKey.toBase58())) {
    throw new Error(
      `Metadata update authority is ${before.updateAuthority}, but the deployer is ${deployer.publicKey.toBase58()}. Only the update authority can rename this mint.`,
    );
  }

  const current = {
    name: clean(before.name),
    symbol: clean(before.symbol),
    uri: clean(before.uri),
  };

  console.log(`Renaming ${artifact.mint} on ${cluster}`);
  console.log(`  name    ${current.name} → ${config.name}`);
  console.log(`  symbol  ${current.symbol} → ${config.symbol}`);
  console.log(`  uri     ${current.uri || "(none)"} → ${config.metadataUri}`);

  if (
    current.name === config.name &&
    current.symbol === config.symbol &&
    current.uri === config.metadataUri
  ) {
    console.log(`\nAlready matches. Nothing to do.`);
    return;
  }

  const { signature } = await updateMetadataAccountV2(umi, {
    metadata: metadataPda,
    updateAuthority: umi.identity,
    // Everything not being changed is carried across explicitly. `data` replaces
    // the whole struct, so omitting a field clears it rather than leaving it.
    data: {
      name: config.name,
      symbol: config.symbol,
      uri: config.metadataUri,
      sellerFeeBasisPoints: before.sellerFeeBasisPoints,
      creators: before.creators,
      collection: before.collection,
      uses: before.uses,
    },
  }).sendAndConfirm(umi, {
    send: { preflightCommitment: "confirmed" },
    confirm: { commitment: "confirmed" },
  });

  const after = await fetchMetadata(umi, metadataPda);
  if (
    clean(after.name) !== config.name ||
    clean(after.symbol) !== config.symbol ||
    clean(after.uri) !== config.metadataUri
  ) {
    throw new Error(
      `Update landed but the account does not match: name=${clean(after.name)} symbol=${clean(after.symbol)} uri=${clean(after.uri)}`,
    );
  }

  writeArtifact(cluster, {
    ...artifact,
    name: config.name,
    symbol: config.symbol,
    metadataPda: new PublicKey(metadataPda[0]).toBase58(),
    metadataUri: config.metadataUri,
    transactions: { ...artifact.transactions, rename: bs58.encode(signature) },
  });

  console.log(`\nVerified on chain. Artifact updated.`);
  console.log(
    `Wallets and explorers cache metadata — run \`pnpm token:verify:${cluster === "devnet" ? "devnet" : "mainnet"}\` to check the truth rather than the cache.`,
  );
  console.log(
    `https://explorer.solana.com/address/${artifact.mint}?cluster=${cluster}`,
  );
};

main().catch((error: unknown) => {
  console.error(`\n${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
