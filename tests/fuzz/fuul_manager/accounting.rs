extern crate std;

use crate::test::accounting_helpers::{actions, exercise};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]
    #[test]
    fn generated_operations_preserve_accounting_and_authorization(
        sequence in proptest::collection::vec(actions(), 1..49),
        compiled in any::<bool>(),
    ) {
        exercise(&sequence, compiled);
    }
}
