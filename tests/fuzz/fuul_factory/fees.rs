extern crate std;

use crate::test::fee_model::{controls, exercise, Step};
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use std::cell::Cell;

#[test]
fn generated_wide_fees_preserve_exact_records_snapshots_events_and_isolation() {
    for guest in [false, true] {
        assert_eq!(exercise(&controls(), guest), 0xfff);
    }
    let native = prop_oneof![
        prop::sample::select(std::vec![i128::MIN, -1, 0, 1, i128::from(u64::MAX), i128::MAX]),
        any::<i128>()
    ];
    let bps = prop_oneof![
        prop::sample::select(std::vec![0, 1, 9_999, 10_000, 10_001, u32::MAX]),
        0_u32..=10_000,
        any::<u32>()
    ];
    let steps =
        prop::collection::vec(
            (0_u8..4, 0_u8..3, 0_u8..8, native, bps)
                .prop_map(|(op, field, index, native, bps)| Step { op, field, index, native, bps }),
            1..25,
        );
    let config = Config { failure_persistence: None, ..Config::default() };
    let cases = config.cases;
    let seen = Cell::new(0);
    TestRunner::new(config)
        .run(&steps, |steps| {
            for guest in [false, true] {
                seen.set(seen.get() | exercise(&steps, guest));
            }
            Ok(())
        })
        .unwrap();
    std::eprintln!("Factory fees: {cases} generated histories x native/guest; generated state mask={:#x}; separate controls cover full i128/BPS boundaries, same-value errors, snapshots and collector history", seen.get());
}
