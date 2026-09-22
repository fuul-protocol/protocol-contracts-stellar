use crate::test::*;

#[test]
fn claim_updates_limits_user_totals_project_inputs_and_fees() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 20_000);
    let check = claim_check(&env, &fixture, &project.address, 100_000, 1);
    let native = TokenClient::new(&env, &fixture.native_asset);
    let claimed_event = Claimed {
        project_address: project.address.clone(),
        to: fixture.recipient.clone(),
        currency: fixture.currency.clone(),
        amount: 100_000,
        currency_type: TokenType::StellarAsset,
        token_id: u(&env, 0),
        reason: ClaimReason::AffiliatePayout,
        proof: check.proof.clone(),
    };
    let claimed_topics: Vec<Val> =
        vec![&env, symbol_short!("claimed").into_val(&env), project.address.clone().into_val(&env)];
    assert_eq!(claimed_event.topics(&env), claimed_topics);

    fixture.client.claim(&fixture.caller, &vec![&env, check.clone()]);
    let claim_events = env.events().all();

    assert_eq!(
        fixture.client.users_claims(&fixture.recipient, &fixture.currency),
        u(&env, 100_000)
    );
    assert_eq!(
        fixture.client.currency_limits(&fixture.currency),
        CurrencyTokenLimit {
            claim_limit_per_cooldown: u(&env, 1_000_000_000_000_i128),
            cumulative_claim_per_cooldown: u(&env, 100_000),
            claim_cooldown_period_started: 1_000_000,
        }
    );
    assert_eq!(project.last_manager(), Some(fixture.client.address.clone()));
    assert_eq!(project.last_recipient(), Some(fixture.recipient.clone()));
    assert!(!project.last_kyc());
    assert_eq!(native.balance(&fixture.caller), 980_000);
    assert_eq!(native.balance(&collector), 20_000);
    assert_eq!(
        claim_events.filter_by_contract(&fixture.client.address).events().last(),
        Some(&claimed_event.to_xdr(&env, &fixture.client.address))
    );
}

#[test]
fn user_claim_totals_are_isolated_by_recipient_and_currency() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    let second_recipient = Address::generate(&env);
    let first = claim_check(&env, &fixture, &project.address, 10, 29);
    let mut native = claim_check(&env, &fixture, &project.address, 20, 30);
    native.currency = fixture.native_asset.clone();
    let mut other_user = claim_check(&env, &fixture, &project.address, 30, 31);
    other_user.to = second_recipient.clone();

    fixture.client.claim(&fixture.caller, &vec![&env, first, native, other_user]);

    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 10));
    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.native_asset), u(&env, 20));
    assert_eq!(fixture.client.users_claims(&second_recipient, &fixture.currency), u(&env, 30));
    assert_eq!(fixture.client.users_claims(&fixture.caller, &fixture.currency), u(&env, 0));
}
