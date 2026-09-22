use crate::test::{constructor_helpers::*, roles_pause_helpers::test_env, *};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]
    #[test]
    fn generated_bootstrap_matches_independent_uniqueness_and_quorum_model(indices in prop::collection::vec(0usize..8, 0..9), quorum in 0u32..10, shared in any::<bool>()) {
        let env = test_env();
        let mut input = Bootstrap::new(&env);
        let actors: std::vec::Vec<_> = (0..8).map(|_| Address::generate(&env)).collect();
        input.signers = indices.iter().map(|i| actors[*i].clone()).fold(Vec::new(&env), |mut list, address| { list.push_back(address); list });
        input.quorum = u128::from(quorum);
        if shared { input.admin = actors[0].clone(); input.pauser = actors[0].clone(); input.unpauser = actors[0].clone(); }
        let unique: std::collections::BTreeSet<_> = indices.iter().copied().collect();
        let expected = if quorum == 0 || indices.is_empty() || quorum as usize > indices.len() { Some(6300) }
            else if unique.len() != indices.len() { Some(6301) } else { None };
        if let Some(code) = expected { reject_native(&env, &input, code); }
        else {
            let id = input.register(&env);
            prop_assert_eq!(env.events().all(), expected_events(&env, &id, &input));
            assert_bootstrap(&env, &id, &input);
        }
    }
}
