//! Leaf is `keccak(0x00 || user || amount_le)`, node is `keccak(0x01 || lo || hi)`.
//! `scripts/tree.ts` has to reproduce both byte for byte.
//!
//! The tags stop a node's digest being read as a leaf. Sorted pairs mean a proof
//! needs no direction bits.

use anchor_lang::prelude::*;
use solana_keccak_hasher::hashv;

const LEAF_TAG: &[u8] = &[0x00];
const NODE_TAG: &[u8] = &[0x01];

/// 24 hashes is 16.7M leaves. Bounded because the proof is caller-supplied.
pub const MAX_PROOF_LEN: usize = 24;

pub fn leaf(user: &Pubkey, amount: u64) -> [u8; 32] {
    hashv(&[LEAF_TAG, user.as_ref(), &amount.to_le_bytes()]).to_bytes()
}

pub fn parent(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    hashv(&[NODE_TAG, lo, hi]).to_bytes()
}

/// Takes `user` and `amount`, never a leaf, so a caller cannot supply a digest the
/// program did not derive.
pub fn verify(root: &[u8; 32], user: &Pubkey, amount: u64, proof: &[[u8; 32]]) -> bool {
    if proof.len() > MAX_PROOF_LEN {
        return false;
    }

    let folded = proof
        .iter()
        .fold(leaf(user, amount), |node, sibling| parent(&node, sibling));

    folded == *root
}

/// Reference builder, host-only — the program only ever verifies. `scripts/tree.ts`
/// must produce the same shape.
#[cfg(not(target_os = "solana"))]
pub mod tree {
    use super::{leaf, parent};
    use anchor_lang::prelude::Pubkey;

    /// Leaves at `levels[0]`, root alone at the top. An unpaired node is promoted
    /// unchanged.
    pub fn levels(allocations: &[(Pubkey, u64)]) -> Vec<Vec<[u8; 32]>> {
        let mut out = vec![allocations
            .iter()
            .map(|(user, amount)| leaf(user, *amount))
            .collect::<Vec<_>>()];

        while out.last().unwrap().len() > 1 {
            let next = out
                .last()
                .unwrap()
                .chunks(2)
                .map(|pair| match pair {
                    [a, b] => parent(a, b),
                    [a] => *a,
                    _ => unreachable!(),
                })
                .collect();
            out.push(next);
        }

        out
    }

    pub fn root(levels: &[Vec<[u8; 32]>]) -> [u8; 32] {
        levels[levels.len() - 1][0]
    }

    pub fn proof(levels: &[Vec<[u8; 32]>], mut index: usize) -> Vec<[u8; 32]> {
        let mut out = vec![];

        for level in &levels[..levels.len() - 1] {
            let sibling = index ^ 1;
            if sibling < level.len() {
                out.push(level[sibling]);
            }
            index /= 2;
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::{tree::*, *};

    fn user(i: usize) -> Pubkey {
        Pubkey::new_from_array([i as u8 + 1; 32])
    }

    fn amount(i: usize) -> u64 {
        (i as u64 + 1) * 1_000_000
    }

    fn allocations(n: usize) -> Vec<(Pubkey, u64)> {
        (0..n).map(|i| (user(i), amount(i))).collect()
    }

    fn levels(n: usize) -> Vec<Vec<[u8; 32]>> {
        super::tree::levels(&allocations(n))
    }

    fn hex(bytes: &[u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Both vectors cross-checked against `@noble/hashes` `keccak_256`, the library
    /// `tree.ts` will use.
    #[test]
    fn byte_layout_is_pinned() {
        assert_eq!(
            hex(&leaf(&user(0), 1_000_000)),
            "cae1413ac2dff2bb360d5995ed928910941436d3322c9545325057d35c9a3a43"
        );
        assert_eq!(
            hex(&parent(&[1; 32], &[2; 32])),
            "5fea46c6ed18832adf4be39bd33e5eeb7cb62ff55182560b78bf99b1220e0ffa"
        );
    }

    /// Same 64 bytes, different tag, different digest.
    #[test]
    fn tags_separate_leaf_and_node_domains() {
        let a = [1u8; 32];
        let b = [2u8; 32];

        assert_ne!(hashv(&[LEAF_TAG, &a, &b]).to_bytes(), parent(&a, &b));
    }

    #[test]
    fn parent_ignores_argument_order() {
        let a = [9u8; 32];
        let b = [4u8; 32];

        assert_eq!(parent(&a, &b), parent(&b, &a));
    }

    #[test]
    fn single_leaf_tree_root_is_the_leaf() {
        let tree = levels(1);

        assert_eq!(root(&tree), leaf(&user(0), amount(0)));
        assert!(verify(&root(&tree), &user(0), amount(0), &[]));
    }

    /// 1 to 9 covers both parities at more than one depth.
    #[test]
    fn every_leaf_verifies_at_every_tree_size() {
        for n in 1..=9 {
            let tree = levels(n);

            for i in 0..n {
                assert!(
                    verify(&root(&tree), &user(i), amount(i), &proof(&tree, i)),
                    "leaf {i} of {n} failed"
                );
            }
        }
    }

    #[test]
    fn wrong_user_fails() {
        let tree = levels(8);

        assert!(!verify(&root(&tree), &user(5), amount(3), &proof(&tree, 3)));
    }

    #[test]
    fn wrong_amount_fails() {
        let tree = levels(8);

        assert!(!verify(
            &root(&tree),
            &user(3),
            amount(3) + 1,
            &proof(&tree, 3)
        ));
    }

    #[test]
    fn stale_root_fails() {
        let old = levels(8);
        let new = levels(9);

        assert!(verify(&root(&old), &user(3), amount(3), &proof(&old, 3)));
        assert!(!verify(&root(&new), &user(3), amount(3), &proof(&old, 3)));
    }

    #[test]
    fn truncated_proof_fails() {
        let tree = levels(8);
        let full = proof(&tree, 3);

        assert!(!verify(
            &root(&tree),
            &user(3),
            amount(3),
            &full[..full.len() - 1]
        ));
    }

    #[test]
    fn extended_proof_fails() {
        let tree = levels(8);
        let mut extended = proof(&tree, 3);
        extended.push([7; 32]);

        assert!(!verify(&root(&tree), &user(3), amount(3), &extended));
    }

    /// Sorted pairs remove direction bits, not ordering: siblings arrive bottom-up.
    #[test]
    fn reordered_proof_fails() {
        let tree = levels(8);
        let mut reversed = proof(&tree, 3);
        reversed.reverse();

        assert!(!verify(&root(&tree), &user(3), amount(3), &reversed));
    }

    #[test]
    fn proof_past_the_cap_fails() {
        let tree = levels(8);

        assert!(!verify(
            &root(&tree),
            &user(3),
            amount(3),
            &vec![[0; 32]; MAX_PROOF_LEN + 1]
        ));
    }
}
