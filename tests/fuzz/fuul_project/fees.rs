extern crate std;

use crate::test::economics::{exercise, Case};
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use std::cell::Cell;

#[test]
fn generated_funding_and_aliases_match_independent_claim_and_removal_arithmetic() {
    for remove in [false, true] {
        for aliases in 0..5 {
            for (amount, claim_bps, remove_bps) in [
                (-1, 0, 0),
                (0, 1, 10_000),
                (1, 9_999, 1),
                (101, 10_000, 9_999),
                (i128::MAX, 10_000, 10_000),
            ] {
                for funding_delta in [-1, 0, 1] {
                    for guest in [false, true] {
                        exercise(
                            Case {
                                remove,
                                amount,
                                claim_bps,
                                remove_bps,
                                quote: i128::MAX,
                                aliases,
                                funding_delta,
                            },
                            guest,
                        );
                    }
                }
            }
        }
    }
    let amounts = prop_oneof![
        prop::sample::select(std::vec![-1, 0, 1, 99, 100, i128::MAX]),
        any::<i128>().prop_map(|n| n & i128::MAX)
    ];
    let bps = prop_oneof![prop::sample::select(std::vec![0, 1, 9_999, 10_000]), 0_u32..=10_000];
    let strategy =
        (any::<bool>(), amounts, bps.clone(), bps, 0_i128..=i128::MAX, 0_u8..5, -1_i8..=1);
    let config = Config { failure_persistence: None, ..Config::default() };
    let cases = config.cases;
    let seen = Cell::new(0);
    TestRunner::new(config)
        .run(&strategy, |(remove, amount, claim_bps, remove_bps, quote, aliases, funding_delta)| {
            for guest in [false, true] {
                seen.set(
                    seen.get()
                        | exercise(
                            Case {
                                remove,
                                amount,
                                claim_bps,
                                remove_bps,
                                quote,
                                aliases,
                                funding_delta,
                            },
                            guest,
                        ),
                );
            }
            Ok(())
        })
        .unwrap();
    std::eprintln!("Project economics: {cases} fresh cases x native/guest; generated mask={:#x}; separate amount/BPS/alias/funding controls passed", seen.get());
}
