use crate::test::{constructor_helpers::*, roles_pause_helpers::test_env, *};

#[test]
fn bootstrap_events_preserve_role_and_currency_input_order() {
    let env = test_env();
    let mut input = Bootstrap::new(&env);
    input.signers.push_back(Address::generate(&env));
    let id = input.register(&env);
    assert_eq!(env.events().all(), expected_events(&env, &id, &input));
    assert_bootstrap(&env, &id, &input);
}

#[test]
fn bootstrap_without_kyc_has_exact_state_and_empty_business_maps() {
    let env = test_env();
    let input = Bootstrap::new(&env);
    let id = input.register(&env);
    assert_bootstrap(&env, &id, &input);
    let client = FuulManagerClient::new(&env, &id);
    assert_eq!(client.kyc_validator(), None);
    assert!(!client.paused());
    for account in [&input.admin, &input.pauser, &input.unpauser] {
        assert!(!client.no_claim_fee_addresses(account));
        assert_eq!(client.users_claims(account, &input.accepted), u(&env, 0));
    }
}

#[test]
fn bootstrap_with_kyc_uses_ledger_seconds_at_timestamp_boundaries() {
    for timestamp in [0, 1_000_007, u64::MAX] {
        let env = test_env();
        env.ledger().with_mut(|ledger| ledger.timestamp = timestamp);
        let mut input = Bootstrap::new(&env);
        input.kyc = Some(Address::generate(&env));
        let id = input.register(&env);
        assert_bootstrap(&env, &id, &input);
        let client = FuulManagerClient::new(&env, &id);
        assert_eq!(client.kyc_validator(), input.kyc);
        assert_eq!(
            client.currency_limits(&input.accepted).claim_cooldown_period_started,
            timestamp
        );
        assert_eq!(client.currency_limits(&input.native).claim_cooldown_period_started, timestamp);
    }
}
