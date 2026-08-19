/**
 * Snapshot -> root + proofs.
 *
 *   pnpm tree <snapshot.json> [out-dir]
 *
 * The snapshot is `{ "<base58>": "<base units>", ... }`, whatever produces it.
 * Keyed by address because that is the whole identity of an allocation — a
 * duplicate wallet cannot even be written down. Writes `root.json` (publish this
 * with `set_root`) and `proofs.json`.
 */

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { type Allocation, build, hex, proof } from "./tree.js";

const [snapshotPath, outDir = "out"] = process.argv.slice(2);

if (!snapshotPath) {
  console.error("usage: pnpm tree <snapshot.json> [out-dir]");
  process.exit(1);
}

const raw: unknown = JSON.parse(readFileSync(snapshotPath, "utf8"));
if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
  throw new Error("snapshot must be a JSON object of address -> base units");
}

const allocations: Allocation[] = Object.entries(raw).map(([user, amount]) => {
  if (typeof amount !== "string" && typeof amount !== "number") {
    throw new Error(`${user}: amount must be a string or number, in base units`);
  }

  return { user, amount: BigInt(amount) };
});

const tree = build(allocations);
const total = allocations.reduce((sum, { amount }) => sum + amount, 0n);

mkdirSync(outDir, { recursive: true });

writeFileSync(
  join(outDir, "root.json"),
  `${JSON.stringify(
    {
      root: hex(tree.root),
      claimants: allocations.length,
      depth: tree.levels.length - 1,
      // Fund the vault with exactly this — see §7 of the scope.
      totalBaseUnits: total.toString(),
    },
    null,
    2,
  )}\n`,
);

writeFileSync(
  join(outDir, "proofs.json"),
  `${JSON.stringify(
    Object.fromEntries(
      tree.allocations.map(({ user, amount }, i) => [
        user,
        { amount: amount.toString(), proof: proof(tree, i).map(hex) },
      ]),
    ),
    null,
    2,
  )}\n`,
);

console.log(`root      ${hex(tree.root)}`);
console.log(`claimants ${allocations.length}`);
console.log(`depth     ${tree.levels.length - 1}`);
console.log(`total     ${total} base units`);
console.log(`written   ${outDir}/root.json, ${outDir}/proofs.json`);
