/**
 * The Merkle tree.
 *
 * `lib/merkle.ts` is the single implementation — the app serves proofs from it
 * too, through the submodule — so a copy here would be a third thing to keep
 * byte-identical with `merkle.rs`, and the fixture only holds two of them
 * together.
 *
 * Re-exported rather than imported directly by every script so the import path
 * stays in one place.
 */

export {
  MAX_PROOF_LEN,
  build,
  hex,
  leaf,
  parent,
  proof,
  type Allocation,
  type Tree,
} from "../../lib/merkle.js";
