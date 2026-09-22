use crate::test::*;
mod security;

#[test]
fn admin_updates_and_disables_the_kyc_validator_with_exact_events() {
    let env = Env::default();
    let fixture = fixture(&env);
    let validator = Address::generate(&env);
    env.mock_all_auths();

    fixture.client.set_kyc_validator(&fixture.admin, &Some(validator.clone()));
    let enabled_event = KycValidatorUpdated { validator: Some(validator.clone()) };
    let enabled_topics: Vec<Val> = vec![
        &env,
        Symbol::new(&env, "kyc_validator_updated").into_val(&env),
        validator.clone().into_val(&env),
    ];
    assert_eq!(enabled_event.topics(&env), enabled_topics);
    assert_eq!(env.events().all(), std::vec![enabled_event.to_xdr(&env, &fixture.client.address)]);
    assert_eq!(fixture.client.kyc_validator(), Some(validator));

    fixture.client.set_kyc_validator(&fixture.admin, &None);
    let disabled_event = KycValidatorUpdated { validator: None };
    let disabled_topics: Vec<Val> =
        vec![&env, Symbol::new(&env, "kyc_validator_updated").into_val(&env), Val::VOID.into()];
    assert_eq!(disabled_event.topics(&env), disabled_topics);
    assert_eq!(env.events().all(), std::vec![disabled_event.to_xdr(&env, &fixture.client.address)]);
    assert_eq!(fixture.client.kyc_validator(), None);
}

#[test]
fn kyc_validator_update_allows_the_current_value_like_the_source() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_kyc_validator(&fixture.admin, &Some(fixture.validator.clone()));

    assert_eq!(fixture.client.kyc_validator(), Some(fixture.validator));
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn kyc_validator_update_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.set_kyc_validator(&fixture.admin, &None);
}

#[test]
fn kyc_validator_result_reaches_the_project_claim() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    assert_eq!(fixture.client.kyc_validator(), None);
    let validator = env.register(MockKyc, (fixture.recipient.clone(),));
    fixture.client.set_kyc_validator(&fixture.admin, &Some(validator));
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);

    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 10, 3)]);

    assert!(project.last_kyc());
}
