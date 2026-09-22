extern crate std;

use crate::test::batch_cases::{exercise, Case};
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use std::cell::Cell;

#[test]
fn generated_small_asset_batches_preserve_atomic_ownership_and_balances() {
    let mut control_late = [0_u32; 2];
    for multi in [false, true] {
        for (ids, quantities, shortage) in [
            (std::vec![], std::vec![], 4),
            (std::vec![1, 2, i128::from(u32::MAX)], std::vec![1, 2, 3], 4),
            (std::vec![1, 2, 3], std::vec![1, 2, 3], 2),
            (std::vec![1, 1], std::vec![6, 6], 4),
            (std::vec![1, i128::from(u32::MAX) + 1], std::vec![1, 1], 4),
            (std::vec![1, -1], std::vec![1, 1], 4),
            (std::vec![1, 2], std::vec![1], 4),
            (std::vec![1], std::vec![-1], 4),
        ] {
            for guest in [false, true] {
                let observed = exercise(
                    &Case { multi, ids: ids.clone(), quantities: quantities.clone(), shortage },
                    guest,
                );
                control_late[usize::from(guest)] += u32::from(observed & (1 << 7) != 0);
            }
        }
    }
    let ids = prop_oneof![
        prop::sample::select(std::vec![
            -1,
            0,
            1,
            2,
            i128::from(u32::MAX),
            i128::from(u32::MAX) + 1
        ]),
        any::<u32>().prop_map(i128::from)
    ];
    let strategy =
        (any::<bool>(), prop::collection::vec((ids, -1_i128..16), 0..5), any::<bool>(), 0_usize..5);
    let config = Config { failure_persistence: None, ..Config::default() };
    let cases = config.cases;
    let seen = Cell::new(0);
    let generated_late = Cell::new([0_u32; 2]);
    TestRunner::new(config)
        .run(&strategy, |(multi, pairs, mismatch, shortage)| {
            let (ids, mut quantities): (std::vec::Vec<_>, std::vec::Vec<_>) =
                pairs.into_iter().unzip();
            if mismatch {
                if quantities.is_empty() {
                    quantities.push(1);
                } else {
                    quantities.pop();
                }
            }
            let case = Case { multi, ids, quantities, shortage };
            for guest in [false, true] {
                let observed = exercise(&case, guest);
                seen.set(seen.get() | observed);
                let mut counts = generated_late.get();
                counts[usize::from(guest)] += u32::from(observed & (1 << 7) != 0);
                generated_late.set(counts);
            }
            Ok(())
        })
        .unwrap();
    assert!(control_late.into_iter().all(|count| count > 0));
    std::eprintln!("Project assets: {cases} fresh <=4-ID batches x native/guest; generated mask={:#x}; observed completed-prefix late failures [native, guest]: controls={control_late:?}, generated={:?}; rollback controls passed", seen.get(), generated_late.get());
}
