extern crate std;

use crate::test::role_model::{controls, exercise};
use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};
use std::cell::Cell;

#[test]
fn generated_role_transitions_bind_each_setter_to_current_authenticated_admins() {
    for guest in [false, true] {
        assert_eq!(exercise(&controls(), guest), 0x7f01ff);
    }
    let config = Config { failure_persistence: None, ..Config::default() };
    let cases = config.cases;
    let seen = Cell::new(0);
    let steps = prop::collection::vec((0_u8..10, 0_u8..3, 0_u8..3, 0_u8..2, 0_u8..3), 1..41);
    TestRunner::new(config)
        .run(&steps, |steps| {
            for guest in [false, true] {
                seen.set(seen.get() | exercise(&steps, guest));
            }
            Ok(())
        })
        .unwrap();
    std::eprintln!("Factory authority: {cases} generated histories x native/guest; generated state mask={:#x}; separate controls cover all seven setters/auth modes/role transitions/last-admin exit", seen.get());
}
