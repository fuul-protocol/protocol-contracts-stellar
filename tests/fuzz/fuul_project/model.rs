extern crate std;
use proptest::prelude::*;

fn config() -> ProptestConfig {
    ProptestConfig { failure_persistence: None, ..ProptestConfig::default() }
}

proptest! {
    #![proptest_config(config())]
    #[test]
    fn generated_project_sequences_match_independent_asset_and_authority_model(
        actions in prop::collection::vec((0_u8..11, 0_u16..800, 0_u8..8, any::<bool>()), 1..20)
    ) {
        for guest in [false, true] {
            crate::test::model::exercise(&actions, guest);
        }
    }
}

#[test]
fn zero_amount_regression_matches_project_model() {
    for guest in [false, true] {
        crate::test::model::exercise(&[(0, 0, 0, false)], guest);
    }
}

#[test]
fn fixed_sequence_covers_dust_roles_kyc_duplicates_and_self_transfers() {
    let actions = [
        (6, 1, 0, false),
        (0, 1, 0, false),
        (0, 0, 1, false),
        (3, 0, 0, false),
        (7, 0, 0, true),
        (0, 3, 2, false),
        (0, 3, 2, true),
        (7, 0, 0, false),
        (9, 0, 0, true),
        (3, 1, 0, true),
        (9, 0, 0, false),
        (3, 1, 0, true),
        (8, 0, 0, false),
        (2, 0, 3, false),
        (8, 0, 0, true),
        (2, 0, 3, true),
        (5, 11, 0, false),
        (5, 2, 0, true),
        (4, 2, 1, false),
        (4, 2, 1, true),
        (1, 0, 4, false),
        (1, 99, 4, true),
        (10, 0, 5, false),
    ];
    for guest in [false, true] {
        crate::test::model::exercise(&actions, guest);
    }
}
