extern crate std;

use crate::test::retention_cases::exercise;
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use std::cell::Cell;

#[test]
fn generated_proof_aging_preserves_selective_renewal_and_replay_rejection() {
    for boundary in 0..6 {
        for query in 0..3 {
            for guest in [false, true] {
                exercise(boundary, query, 11, [71; 32], guest);
            }
        }
    }
    let config = Config { failure_persistence: None, ..Config::default() };
    let cases = config.cases;
    let seen = Cell::new(0);
    TestRunner::new(config)
        .run(
            &(0_u8..6, 0_u8..3, 1_u32..65, any::<[u8; 32]>()),
            |(boundary, query, extra, proof)| {
                for guest in [false, true] {
                    seen.set(seen.get() | exercise(boundary, query, extra, proof, guest));
                }
                Ok(())
            },
        )
        .unwrap();
    std::eprintln!("Project retention: {cases} fresh claim/age/read/replay cases x native/guest; generated mask={:#x}; separate threshold/live-until/expired controls passed (SDK restoration only)", seen.get());
}
