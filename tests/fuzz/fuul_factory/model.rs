extern crate std;

use crate::test::model::{exercise, Action};
use proptest::prelude::*;

fn actions() -> impl Strategy<Value = Action> {
    let values = prop::sample::select(std::vec![0_u32, 1, 100, 9_999, 10_000, 10_001, 20_000]);
    prop_oneof![
        3 => any::<bool>().prop_map(Action::Create),
        3 => (0_u8..3, values.clone()).prop_map(|(field, value)| Action::Default(field, value)),
        3 => (0_u8..8, 0_u8..3, values).prop_map(|(index, field, value)| Action::Override(index, field, value)),
        2 => (0_u8..3).prop_map(Action::Collector),
        2 => (0_u8..8).prop_map(Action::Read),
        1 => Just(Action::Denied),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]
    #[test]
    fn generated_factory_sequences_match_snapshot_and_isolation_model(sequence in prop::collection::vec(actions(), 1..17)) {
        for compiled in [false, true] { exercise(&sequence, compiled); }
    }
}

#[test]
fn zero_default_fee_regression_matches_factory_model() {
    for compiled in [false, true] {
        exercise(&[Action::Default(0, 0)], compiled);
    }
}
