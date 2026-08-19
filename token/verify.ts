/**
 * Checks that the token presents correctly — on chain and over HTTP.
 *
 *   pnpm token:verify:devnet
 *
 * Read-only, and the thing to run after `token:rename` rather than opening a
 * wallet. Phantom, Solscan and Jupiter all cache token metadata for hours, so a
 * correct update routinely still looks wrong in a wallet long after it landed.
 * This reads the account and fetches the document, which is what the caches are
 * a stale copy of.
 *
 * It checks three separate things, because they fail independently:
 *
 *   the account   — name, symbol and uri as the chain has them
 *   the document  — does `uri` actually resolve, and is it the right shape
 *   the image     — does `image` resolve, and is it really an image
 *
 * A token with perfect on-chain fields and a dead `uri` renders as a nameless
 * blank in every wallet, and nothing on chain would tell you.
 */

import { getMint } from "@solana/spl-token";
import { Connection, PublicKey } from "@solana/web3.js";

import {
  fetchMetadata,
  findMetadataPda,
  mplTokenMetadata,
} from "@metaplex-foundation/mpl-token-metadata";
import { publicKey } from "@metaplex-foundation/umi";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";

import { parseCluster, readArtifact } from "./common";
import { CLUSTERS, TOKEN_DECIMALS } from "./config";

const clean = (value: string) => value.replace(/\0+$/, "");

let failed = false;

const check = (ok: boolean, label: string, detail: string) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label.padEnd(22)} ${detail}`);
  if (!ok) failed = true;
};

/** A fetch that reports why it failed instead of throwing a bare TypeError. */
const get = async (url: string) => {
  try {
    const response = await fetch(url, { redirect: "follow" });
    return { response, error: null as string | null };
  } catch (error) {
    return {
      response: null,
      error: error instanceof Error ? error.message : String(error),
    };
  }
};

const main = async () => {
  const cluster = parseCluster();
  const config = CLUSTERS[cluster];
  const artifact = readArtifact(cluster);
  const connection = new Connection(config.rpcUrl, "confirmed");

  console.log(`${artifact.mint} on ${cluster}\n`);

  console.log("mint");
  const mint = await getMint(connection, new PublicKey(artifact.mint));
  check(mint.decimals === TOKEN_DECIMALS, "decimals", `${mint.decimals}`);
  check(mint.mintAuthority === null, "mint authority", mint.mintAuthority ? `${mint.mintAuthority}` : "revoked");
  check(mint.freezeAuthority === null, "freeze authority", mint.freezeAuthority ? `${mint.freezeAuthority}` : "never granted");
  check(
    mint.supply.toString() === artifact.totalSupplyBaseUnits,
    "supply",
    `${mint.supply}${mint.supply.toString() === artifact.totalSupplyBaseUnits ? "" : ` (artifact says ${artifact.totalSupplyBaseUnits})`}`,
  );

  const umi = createUmi(connection).use(mplTokenMetadata());
  const metadata = await fetchMetadata(
    umi,
    findMetadataPda(umi, { mint: publicKey(artifact.mint) }),
  );
  const onChain = {
    name: clean(metadata.name),
    symbol: clean(metadata.symbol),
    uri: clean(metadata.uri),
  };

  console.log("\naccount");
  check(onChain.name === config.name, "name", `${onChain.name}${onChain.name === config.name ? "" : ` (config says ${config.name})`}`);
  check(onChain.symbol === config.symbol, "symbol", `${onChain.symbol}${onChain.symbol === config.symbol ? "" : ` (config says ${config.symbol})`}`);
  check(onChain.uri === config.metadataUri, "uri", onChain.uri || "(empty)");
  check(metadata.isMutable, "mutable", metadata.isMutable ? "yes — renameable" : "no — name is permanent");
  check(
    onChain.name === artifact.name && onChain.symbol === artifact.symbol,
    "artifact agrees",
    onChain.name === artifact.name && onChain.symbol === artifact.symbol
      ? "yes"
      : `no — artifact has ${artifact.name} / ${artifact.symbol}, so the app will render the wrong ticker`,
  );

  console.log("\ndocument");
  if (!onChain.uri) {
    check(false, "fetch", "no uri on chain — nothing to fetch");
  } else if (!onChain.uri.startsWith("http")) {
    check(false, "scheme", `${onChain.uri.split(":")[0]}: — Solana wallets fetch this over HTTP and will not resolve it`);
  } else {
    const { response, error } = await get(onChain.uri);
    if (!response) {
      check(false, "fetch", `unreachable — ${error}`);
    } else {
      check(response.ok, "fetch", `HTTP ${response.status}`);
      if (response.ok) {
        const body = (await response.json().catch(() => null)) as Record<string, unknown> | null;
        check(body !== null, "json", body ? "parsed" : "not valid JSON");

        if (body) {
          check(body.name === config.name, "name", `${String(body.name)}`);
          check(body.symbol === config.symbol, "symbol", `${String(body.symbol)}`);
          check(typeof body.description === "string" && body.description.length > 0, "description", typeof body.description === "string" ? `${body.description.length} chars` : "missing");

          const image = typeof body.image === "string" ? body.image : "";
          console.log("\nimage");
          if (!image) {
            check(false, "present", "no `image` field — the token renders blank");
          } else if (!image.startsWith("http")) {
            check(false, "scheme", `${image.split(":")[0]}: — most wallets will not resolve it`);
          } else {
            const found = await get(image);
            if (!found.response) {
              check(false, "fetch", `unreachable — ${found.error}`);
            } else {
              const type = found.response.headers.get("content-type") ?? "";
              const length = Number(found.response.headers.get("content-length") ?? 0);
              check(found.response.ok, "fetch", `HTTP ${found.response.status}`);
              check(type.startsWith("image/"), "content-type", type || "(none)");
              check(length > 1024, "size", length ? `${(length / 1024).toFixed(0)} KB` : "unknown");
            }
          }
        }
      }
    }
  }

  console.log(
    `\n${failed ? "FAILED — see ✗ above." : "All good."}\nhttps://explorer.solana.com/address/${artifact.mint}?cluster=${cluster}`,
  );
  if (failed) process.exit(1);
};

main().catch((error: unknown) => {
  console.error(`\n${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
