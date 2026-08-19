/**
 * Merkle tree for `sleepagotchi_airdrop`.
 *
 * Must stay byte-identical to `programs/sleepagotchi_airdrop/src/merkle.rs`.
 * `scripts/claim/fixture.ts` writes a fixture that a Rust test checks against,
 * so a divergence here fails `cargo test` rather than the airdrop.
 *
 * The one implementation, used by three callers: `scripts/claim` builds roots
 * and fixtures with it, the Rust verifier is checked against it, and the app
 * serves proofs from it through the submodule. A copy on either side would be a
 * third thing to keep identical.
 *
 * Pure and dependency-free apart from hashing, so it runs anywhere — a browser,
 * a script, a test — with nothing configured.
 */

import { keccak_256 } from "@noble/hashes/sha3";
import bs58 from "bs58";

const LEAF_TAG = 0x00;
const NODE_TAG = 0x01;

/** Matches `MAX_PROOF_LEN` in merkle.rs. */
export const MAX_PROOF_LEN = 24;

export interface Allocation {
  /** Base58 Solana address. */
  user: string;
  /** Base units. `bigint` because a u64 allocation overflows `number`. */
  amount: bigint;
}

export interface Tree {
  root: Uint8Array;
  /** Bottom-up: leaves at `levels[0]`, root alone at the top. */
  levels: Uint8Array[][];
  /** Canonical order — sorted by address, not the order they were passed in. */
  allocations: Allocation[];
}

export function leaf(user: string, amount: bigint): Uint8Array {
  const address = bs58.decode(user);
  if (address.length !== 32) {
    throw new Error(`${user} is not a 32-byte address`);
  }

  const preimage = new Uint8Array(1 + 32 + 8);
  preimage[0] = LEAF_TAG;
  preimage.set(address, 1);
  preimage.set(u64le(amount), 33);

  return keccak_256(preimage);
}

export function parent(a: Uint8Array, b: Uint8Array): Uint8Array {
  const [lo, hi] = compare(a, b) <= 0 ? [a, b] : [b, a];

  const preimage = new Uint8Array(1 + 32 + 32);
  preimage[0] = NODE_TAG;
  preimage.set(lo, 1);
  preimage.set(hi, 33);

  return keccak_256(preimage);
}

/**
 * Sorted by wallet address, so the root is a function of the set of allocations and
 * not of the order they arrived in. Pairing is positional — only each pair is
 * hashed sorted — so without this a reordered snapshot gives a different root and
 * nobody can independently reproduce it.
 *
 * Duplicate wallets are rejected rather than summed: a wallet can only ever claim
 * one leaf, so a second would be silently unclaimable, and guessing which was
 * intended is worse than stopping.
 */
export function build(allocations: Allocation[]): Tree {
  if (allocations.length === 0) {
    throw new Error("cannot build a tree with no allocations");
  }

  // Decode once, then sort on bytes — decoding inside the comparator would be
  // O(n log n) base58 decodes.
  const decoded = allocations.map(({ user, amount }) => {
    const address = bs58.decode(user);
    if (address.length !== 32) {
      throw new Error(`${user} is not a 32-byte address`);
    }
    if (amount <= 0n) {
      throw new Error(`${user} has a non-positive allocation`);
    }

    return { user, amount, address };
  });

  decoded.sort((a, b) => compare(a.address, b.address));

  for (let i = 1; i < decoded.length; i++) {
    // Sorted, so duplicates are adjacent.
    if (compare(decoded[i - 1].address, decoded[i].address) === 0) {
      throw new Error(`${decoded[i].user} appears more than once`);
    }
  }

  const levels: Uint8Array[][] = [decoded.map(({ user, amount }) => leaf(user, amount))];

  while (levels[levels.length - 1].length > 1) {
    const below = levels[levels.length - 1];
    const next: Uint8Array[] = [];

    for (let i = 0; i < below.length; i += 2) {
      // An unpaired node is promoted unchanged — the convention merkle.rs uses.
      next.push(i + 1 < below.length ? parent(below[i], below[i + 1]) : below[i]);
    }

    levels.push(next);
  }

  const depth = levels.length - 1;
  if (depth > MAX_PROOF_LEN) {
    throw new Error(`tree is ${depth} deep, past the program's ${MAX_PROOF_LEN}`);
  }

  return {
    root: levels[levels.length - 1][0],
    levels,
    allocations: decoded.map(({ user, amount }) => ({ user, amount })),
  };
}

export function proof(tree: Tree, index: number): Uint8Array[] {
  const out: Uint8Array[] = [];

  for (let level = 0; level < tree.levels.length - 1; level++) {
    const sibling = index ^ 1;
    if (sibling < tree.levels[level].length) {
      out.push(tree.levels[level][sibling]);
    }
    index = Math.floor(index / 2);
  }

  return out;
}

export function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function u64le(value: bigint): Uint8Array {
  if (value < 0n || value > 0xffffffffffffffffn) {
    throw new Error(`${value} does not fit in a u64`);
  }

  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, value, true);

  return bytes;
}

function compare(a: Uint8Array, b: Uint8Array): number {
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return a[i] < b[i] ? -1 : 1;
  }

  return 0;
}
