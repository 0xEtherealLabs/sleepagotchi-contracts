/**
 * Writes `fixtures/tree.json`, which a Rust test in `sleepagotchi_airdrop` checks
 * against its own builder and verifier.
 *
 * Deterministic, so regenerating without changing the algorithm produces no diff.
 * Run `pnpm tree:fixture` after touching `tree.ts`; a real change to the byte
 * layout or the tree shape shows up as a failing `cargo test`.
 */

import { keccak_256 } from "@noble/hashes/sha3";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import bs58 from "bs58";

import { type Allocation, build, hex, proof } from "./tree.js";

/** Sizes either side of each power of two — where unpaired-node promotion bites. */
const SIZES = [1, 2, 3, 4, 5, 7, 8, 9, 16, 17, 33];

const U64_MAX = 18446744073709551615n;

const OUT = join(import.meta.dirname, "..", "..", "fixtures", "tree.json");

/** Hashed so byte ordering varies, which exercises both sides of the sorted pair. */
function address(seed: string): string {
  return bs58.encode(keccak_256(new TextEncoder().encode(seed)));
}

function allocations(n: number): Allocation[] {
  return Array.from({ length: n }, (_, i) => ({
    user: address(`sleepagotchi-fixture-${n}-${i}`),
    // Last leaf of the largest tree carries u64::MAX, so a mismatch in the
    // little-endian amount encoding cannot hide in small numbers.
    amount: n === 33 && i === n - 1 ? U64_MAX : BigInt(i + 1) * 1_000_000_000n + BigInt(i),
  }));
}

/** Deterministic reorderings — a build that does not sort will disagree. */
function reorderings(list: Allocation[]): Allocation[][] {
  return [
    [...list].reverse(),
    [...list.slice(1), ...list.slice(0, 1)],
    [...list].sort((a, b) => a.user.localeCompare(b.user)),
  ];
}

const cases = SIZES.map((n) => {
  const tree = build(allocations(n));

  // The root must be a function of the set, not the order it arrived in.
  for (const reordered of reorderings(allocations(n))) {
    const other = build(reordered);
    if (hex(other.root) !== hex(tree.root)) {
      throw new Error(`n=${n}: root depends on input order`);
    }
  }

  return {
    n,
    root: hex(tree.root),
    depth: tree.levels.length - 1,
    allocations: tree.allocations.map(({ user, amount }) => ({
      user,
      userHex: hex(bs58.decode(user)),
      amount: amount.toString(),
    })),
    proofs: tree.allocations.map((_, i) => proof(tree, i).map(hex)),
  };
});

mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(
  OUT,
  `${JSON.stringify(
    {
      generatedBy: "contracts/scripts/claim/fixture.ts — pnpm tree:fixture",
      leafTag: 0,
      nodeTag: 1,
      cases,
    },
    null,
    2,
  )}\n`,
);

console.log(`wrote ${cases.length} cases to ${OUT}`);
