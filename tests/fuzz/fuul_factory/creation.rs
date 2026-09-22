use crate::test::{creation_helpers::MAX_TRACKER, deployment_model::exercise};
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use std::cell::Cell;

#[test]
fn generated_counter_deployment_failures_preserve_address_snapshot_and_retry() {
    let boundaries = [
        0,
        u128::from(u64::MAX) - 1,
        u128::from(u64::MAX),
        u128::from(u64::MAX) + 1,
        MAX_TRACKER - 1,
        MAX_TRACKER,
    ];
    for counter in boundaries {
        for guest in [false, true] {
            exercise(counter, [19; 32], 0, guest);
        }
    }
    for mode in 1..5 {
        for guest in [false, true] {
            exercise(MAX_TRACKER, [27; 32], mode, guest);
        }
    }
    let counters = prop_oneof![2 => prop::sample::select(boundaries.to_vec()), 3 => any::<u128>().prop_map(|n| n & MAX_TRACKER)];
    let config = Config { failure_persistence: None, ..Config::default() };
    let cases = config.cases;
    let seen = Cell::new(0);
    TestRunner::new(config)
        .run(&(counters, any::<[u8; 32]>(), 0_u8..5), |(counter, network, mode)| {
            for guest in [false, true] {
                seen.set(seen.get() | exercise(counter, network, mode, guest));
            }
            Ok(())
        })
        .unwrap();
    std::eprintln!("Factory creation: {cases} generated cases x native/guest; generated state mask={:#x} (bits 0..4 failure modes, 5 wide counter, 6 wrap); separate boundary controls passed", seen.get());
}
extern crate std;
