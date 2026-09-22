extern crate std;

use crate::test::retention_model::{exercise, Case};
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use std::cell::Cell;

#[test]
fn generated_fee_reads_renew_only_selected_live_entries_and_restore_archived_records() {
    let mut controls = 0;
    for delta in [-1, 0, 1] {
        for query in 0..9 {
            let case = Case {
                delta,
                query,
                values: if query % 2 == 0 { [0; 3] } else { [i128::MAX, 10_000, 9_999] },
                absent: false,
                archive: query == 1,
                extra: 1,
            };
            for guest in [false, true] {
                controls |= exercise(case, guest);
            }
        }
    }
    for query in 0..2 {
        for guest in [false, true] {
            controls |= exercise(
                Case { delta: 0, query, values: [7, 0, 0], absent: true, archive: true, extra: 2 },
                guest,
            );
        }
    }
    assert_eq!(controls, 0xffff);
    let native = prop_oneof![Just(0), Just(i128::MAX), 0_i128..=i128::MAX];
    let cases_strategy = (
        -1_i8..=1,
        0_u8..9,
        native,
        0_u32..=10_000,
        0_u32..=10_000,
        any::<bool>(),
        any::<bool>(),
        1_u32..65,
    );
    let config = Config { failure_persistence: None, ..Config::default() };
    let cases = config.cases;
    let seen = Cell::new(0);
    TestRunner::new(config)
        .run(&cases_strategy, |(delta, query, native, claim, removal, absent, archive, extra)| {
            let case = Case {
                delta,
                query,
                values: [native, i128::from(claim), i128::from(removal)],
                absent,
                archive,
                extra,
            };
            for guest in [false, true] {
                seen.set(seen.get() | exercise(case, guest));
            }
            Ok(())
        })
        .unwrap();
    std::eprintln!("Factory retention: {cases} generated cases x native/guest; generated state mask={:#x}; separate controls cover nine reads, threshold -1/equal/+1, zero/absent/archived records", seen.get());
}
