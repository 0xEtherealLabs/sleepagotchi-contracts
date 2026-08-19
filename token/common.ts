import { readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { Keypair } from "@solana/web3.js";

import { CLUSTERS, type ClusterName } from "./config";

export const parseCluster = (): ClusterName => {
  const name = process.argv[2];
  if (!name || !(name in CLUSTERS)) {
    throw new Error(`Pass a cluster: ${Object.keys(CLUSTERS).join(" | ")}`);
  }
  return name as ClusterName;
};

/**
 * The only thing taken from the environment — it's a secret and a path.
 * Solana CLI format: a JSON array of the 64 secret key bytes.
 */
export const loadDeployer = () => {
  const path = process.env.SOLANA_DEPLOYER_KEYPAIR || "~/.config/solana/id.json";
  const expanded = path.startsWith("~/")
    ? resolve(homedir(), path.slice(2))
    : resolve(path);

  try {
    return Keypair.fromSecretKey(
      Uint8Array.from(JSON.parse(readFileSync(expanded, "utf8"))),
    );
  } catch {
    throw new Error(
      `No usable Solana keypair at ${expanded}. Create one with \`solana-keygen new\`, or set SOLANA_DEPLOYER_KEYPAIR.`,
    );
  }
};

export type Artifact = {
  cluster: ClusterName;
  mint: string;
  name: string;
  symbol: string;
  decimals: number;
  totalSupplyBaseUnits: string;
  metadataPda: string;
  metadataUri: string;
  authorities: { mint: null; freeze: null; metadataUpdate: string };
  treasury: { owner: string; tokenAccount: string };
  deployer: string;
  deployedAt: string;
  transactions: Record<string, string>;
};

const dir = join(dirname(fileURLToPath(import.meta.url)), "deployments");

export const artifactPath = (cluster: ClusterName) =>
  join(dir, `${cluster}.json`);

export const readArtifact = (cluster: ClusterName): Artifact => {
  try {
    return JSON.parse(readFileSync(artifactPath(cluster), "utf8"));
  } catch {
    throw new Error(`No deployment on ${cluster} — deploy first.`);
  }
};

export const writeArtifact = (cluster: ClusterName, artifact: Artifact) => {
  mkdirSync(dir, { recursive: true });
  writeFileSync(artifactPath(cluster), `${JSON.stringify(artifact, null, 2)}\n`);
};

/** Decimal string to base units, without ever touching a float. */
export const toBaseUnits = (amount: string, decimals: number) => {
  if (!/^\d+(\.\d+)?$/.test(amount)) {
    throw new Error(`Invalid amount "${amount}" — expected a positive number.`);
  }
  const [whole, fraction = ""] = amount.split(".");
  if (fraction.length > decimals) {
    throw new Error(`"${amount}" has more than ${decimals} decimal places.`);
  }
  return BigInt(whole + fraction.padEnd(decimals, "0"));
};

export const toDisplay = (base: bigint, decimals: number) => {
  const unit = 10n ** BigInt(decimals);
  const fraction = (base % unit)
    .toString()
    .padStart(decimals, "0")
    .replace(/0+$/, "");
  return fraction ? `${base / unit}.${fraction}` : `${base / unit}`;
};
