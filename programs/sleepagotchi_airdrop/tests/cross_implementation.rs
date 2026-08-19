//! Checks `scripts/tree.ts` against this program's verifier.
//!
//! The fixture is committed, so this runs on `cargo test` alone with no node
//! toolchain. Regenerate it with `pnpm tree:fixture` after touching `tree.ts`; a
//! real divergence in byte layout or tree shape fails here.

use anchor_lang::prelude::Pubkey;
use sleepagotchi_airdrop::merkle::{self, tree};

const FIXTURE: &str = include_str!("../../../fixtures/tree.json");

fn from_hex(text: &str) -> [u8; 32] {
    let bytes: Vec<u8> = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect();

    bytes.try_into().expect("expected 32 bytes")
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Same allocations must give the same root, the same depth, and the same proof
/// siblings in the same order — not merely two proofs that both happen to verify.
#[test]
fn agrees_with_the_typescript_builder() {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    let cases = fixture["cases"].as_array().expect("cases");

    assert!(!cases.is_empty(), "fixture has no cases");

    for case in cases {
        let n = case["n"].as_u64().unwrap();
        let allocations: Vec<(Pubkey, u64)> = case["allocations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                (
                    Pubkey::new_from_array(from_hex(entry["userHex"].as_str().unwrap())),
                    entry["amount"].as_str().unwrap().parse().unwrap(),
                )
            })
            .collect();

        let levels = tree::levels(&allocations);
        let root = tree::root(&levels);

        assert_eq!(hex(&root), case["root"].as_str().unwrap(), "root at n={n}");
        assert_eq!(
            levels.len() - 1,
            case["depth"].as_u64().unwrap() as usize,
            "depth at n={n}"
        );

        for (i, (user, amount)) in allocations.iter().enumerate() {
            let theirs: Vec<[u8; 32]> = case["proofs"][i]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| from_hex(h.as_str().unwrap()))
                .collect();
            let ours = tree::proof(&levels, i);

            assert_eq!(
                theirs.iter().map(hex).collect::<Vec<_>>(),
                ours.iter().map(hex).collect::<Vec<_>>(),
                "proof at n={n} index={i}"
            );

            // The bytes TypeScript produced, through the verifier the program runs.
            assert!(
                merkle::verify(&root, user, *amount, &theirs),
                "verifier rejected the typescript proof at n={n} index={i}"
            );
        }
    }
}

/// `tree.ts` sorts by address so the root is a function of the set rather than of
/// the snapshot's ordering. Pairing is positional, so dropping that sort would
/// silently make the root unreproducible — this is what catches it.
#[test]
fn fixture_allocations_are_in_canonical_address_order() {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();

    for case in fixture["cases"].as_array().unwrap() {
        let addresses: Vec<[u8; 32]> = case["allocations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| from_hex(entry["userHex"].as_str().unwrap()))
            .collect();

        for pair in addresses.windows(2) {
            assert!(
                pair[0] < pair[1],
                "allocations are not strictly ascending at n={}",
                case["n"]
            );
        }
    }
}

/// Guards against a vacuous fixture: the proofs have to be index-specific.
#[test]
fn typescript_proofs_do_not_verify_for_the_wrong_leaf() {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();

    for case in fixture["cases"].as_array().unwrap() {
        let allocations = case["allocations"].as_array().unwrap();
        if allocations.len() < 2 {
            continue;
        }

        let root = from_hex(case["root"].as_str().unwrap());
        let first_proof: Vec<[u8; 32]> = case["proofs"][0]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| from_hex(h.as_str().unwrap()))
            .collect();

        let other = Pubkey::new_from_array(from_hex(allocations[1]["userHex"].as_str().unwrap()));
        let amount: u64 = allocations[1]["amount"].as_str().unwrap().parse().unwrap();

        assert!(
            !merkle::verify(&root, &other, amount, &first_proof),
            "leaf 0's proof verified for leaf 1 at n={}",
            case["n"]
        );
    }
}
