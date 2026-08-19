/**
 * Uploads the token image and its metadata document to IPFS, and prints the URL
 * to paste into `metadataUri` in `config.ts`.
 *
 *   pnpm token:upload:devnet
 *
 * Two uploads rather than one: the document has to name the image, so the image
 * needs a URL before the document can be written.
 *
 * What goes on chain is the HTTPS gateway URL, not `ipfs://`. Solana wallets and
 * explorers fetch `uri` over plain HTTP and most of them do not dereference the
 * ipfs scheme, so an `ipfs://` uri renders as a token with no image in exactly
 * the places that matter. The CID is printed alongside because that is the
 * durable half — the gateway is just one door onto it.
 *
 * Nothing here touches the chain. Uploading is safe to repeat; identical bytes
 * pin to the same CID, so re-running without changing anything is a no-op that
 * prints the same URL.
 */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { createThirdwebClient } from "thirdweb";
import { resolveScheme, upload } from "thirdweb/storage";

import { parseCluster } from "./common";
import { CLUSTERS, METADATA } from "./config";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");

const requireEnv = (name: string) => {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is not set. It lives in .env — see .env.example.`);
  }
  return value;
};

/** `upload` returns a string for one file and an array for several. */
const one = (result: string | string[]) =>
  Array.isArray(result) ? result[0]! : result;

const main = async () => {
  const cluster = parseCluster();
  const config = CLUSTERS[cluster];

  const client = createThirdwebClient({
    // Both: the secret authorizes the upload, the client id is what the gateway
    // subdomain is keyed by. Deriving one from the other is thirdweb's business
    // and not something to depend on here.
    clientId: requireEnv("NEXT_PUBLIC_THIRDWEB_CLIENT_ID"),
    secretKey: requireEnv("THIRDWEB_SECRET_KEY"),
  });

  const imagePath = join(repoRoot, METADATA.image);
  const image = readFileSync(imagePath);
  console.log(`Uploading ${METADATA.image} (${(image.length / 1024).toFixed(0)} KB)`);

  const imageUri = one(
    await upload({
      client,
      files: [{ data: new Uint8Array(image), name: "sleep.png" }],
      // Without this the file lands inside a directory and the URI gains a
      // trailing `/0`, which some gateways serve as a listing rather than a PNG.
      uploadWithoutDirectory: true,
    }),
  );
  const imageUrl = resolveScheme({ client, uri: imageUri });
  console.log(`  ${imageUri}`);

  /**
   * Metaplex's fungible-token shape. `name`, `symbol` and `image` are what every
   * wallet reads; `properties.files` is what the stricter explorers want, and
   * costs nothing to include.
   */
  const document = {
    name: config.name,
    symbol: config.symbol,
    description: METADATA.description,
    image: imageUrl,
    external_url: METADATA.externalUrl,
    properties: {
      files: [{ uri: imageUrl, type: "image/png" }],
      category: "image",
    },
  };

  console.log(`\nUploading metadata document for ${config.symbol} on ${cluster}`);
  /*
   * Serialised here and uploaded as bytes, rather than handed over as an object.
   *
   * Given an object, thirdweb walks it and rewrites any gateway URL it
   * recognises back to `ipfs://` — the right call on EVM, and precisely wrong
   * here, because it silently undoes the scheme choice above and leaves the
   * image unresolvable in the wallets this is for. Bytes are passed through
   * untouched.
   */
  const documentUri = one(
    await upload({
      client,
      files: [
        {
          data: new TextEncoder().encode(JSON.stringify(document, null, 2)),
          name: "sleep.json",
        },
      ],
      uploadWithoutDirectory: true,
    }),
  );
  const documentUrl = resolveScheme({ client, uri: documentUri });
  console.log(`  ${documentUri}`);

  console.log(`\nimage     ${imageUrl}`);
  console.log(`metadata  ${documentUrl}`);
  console.log(
    `\nPaste into CLUSTERS["${cluster}"].metadataUri in token/config.ts:\n\n    metadataUri: "${documentUrl}",\n\nThen \`pnpm token:rename:${cluster === "devnet" ? "devnet" : "mainnet"}\` to put it on chain.`,
  );
};

main().catch((error: unknown) => {
  console.error(`\n${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
