extern crate std;

use crate::test::authorization_cases::{exercise, Case};
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use std::cell::Cell;

#[test]
fn generated_authorization_and_proofs_preserve_precise_errors_and_project_scope() {
    for scenario in 0..7 {
        for scope in 0..4 {
            for guest in [false, true] {
                exercise(Case { scenario, scope, amount: 11, proof: [61; 32] }, guest);
            }
        }
    }
    let config = Config { failure_persistence: None, ..Config::default() };
    let cases = config.cases;
    let seen = Cell::new(0);
    TestRunner::new(config)
        .run(
            &(0_u8..7, 0_u8..4, 1_u16..101, any::<[u8; 32]>()),
            |(scenario, scope, amount, proof)| {
                for guest in [false, true] {
                    seen.set(seen.get() | exercise(Case { scenario, scope, amount, proof }, guest));
                }
                Ok(())
            },
        )
        .unwrap();
    std::eprintln!("Project authorization: {cases} fresh cases with fixed reject/claim/replay/second-Project transitions x native/guest; generated mask={:#x}; separate precondition/scope controls passed", seen.get());
}
