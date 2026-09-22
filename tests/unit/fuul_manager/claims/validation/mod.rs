use crate::test::*;

#[test]
fn non_fungible_zero_claim_uses_an_unconfigured_currency_default() {
    for timestamp in [0, 86_400] {
        let env = Env::default();
        let fixture = claim_fixture(&env);
        env.ledger().with_mut(|ledger| ledger.timestamp = timestamp);
        let collector = Address::generate(&env);
        let project = register_mock_project(&env, &collector, 20_000);
        let mut check = claim_check(&env, &fixture, &project.address, 0, 40);
        check.currency = Address::generate(&env);
        check.currency_type = TokenType::NonFungible;
        check.token_id = u(&env, 1);

        let mut positive = check.clone();
        positive.amount = 1;
        assert_eq!(
            fixture.client.try_claim(&fixture.caller, &vec![&env, positive]),
            Err(Ok(Error::from_contract_error(6304)))
        );
        assert_eq!(project.last_recipient(), None);
        fixture.client.claim(&fixture.caller, &vec![&env, check.clone()]);
        assert_eq!(project.last_recipient(), Some(fixture.recipient.clone()));
        assert_eq!(
            fixture.client.currency_limits(&check.currency),
            CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, 0),
                cumulative_claim_per_cooldown: u(&env, 0),
                claim_cooldown_period_started: timestamp,
            }
        );
        assert_eq!(fixture.client.users_claims(&fixture.recipient, &check.currency), u(&env, 0));
    }
}

#[test]
fn multi_token_zero_claim_uses_an_unconfigured_currency_default() {
    for timestamp in [0, 86_400] {
        let env = Env::default();
        let fixture = claim_fixture(&env);
        env.ledger().with_mut(|ledger| ledger.timestamp = timestamp);
        let project = register_mock_project(&env, &Address::generate(&env), 0);
        let mut check = claim_check(&env, &fixture, &project.address, 0, 41);
        check.currency = Address::generate(&env);
        check.currency_type = TokenType::MultiToken;
        check.token_id = u(&env, 1);

        let mut positive = check.clone();
        positive.amount = 1;
        assert_eq!(
            fixture.client.try_claim(&fixture.caller, &vec![&env, positive]),
            Err(Ok(Error::from_contract_error(6304)))
        );
        assert_eq!(project.last_recipient(), None);
        fixture.client.claim(&fixture.caller, &vec![&env, check.clone()]);
        assert_eq!(project.last_recipient(), Some(fixture.recipient.clone()));
        assert_eq!(
            fixture.client.currency_limits(&check.currency),
            CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, 0),
                cumulative_claim_per_cooldown: u(&env, 0),
                claim_cooldown_period_started: timestamp,
            }
        );
        assert_eq!(fixture.client.users_claims(&fixture.recipient, &check.currency), u(&env, 0));
    }
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn claim_rejects_a_negative_amount_before_soroban_math() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);

    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, -1, 17)]);
}

#[test]
fn claim_rejects_a_negative_token_id() {
    use soroban_sdk::{Map, TryFromVal};
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    let check = claim_check(&env, &fixture, &project.address, 10, 18);
    let value: Val = check.into_val(&env);
    let mut fields = Map::<Symbol, Val>::try_from_val(&env, &value).unwrap();
    fields.set(Symbol::new(&env, "token_id"), (-1_i128).into_val(&env));
    let before = security_helpers::state(&env, &fixture.client.address);
    assert_eq!(
        env.try_invoke_contract::<(), Error>(
            &fixture.client.address,
            &symbol_short!("claim"),
            (&fixture.caller, vec![&env, fields]).into_val(&env)
        ),
        Err(Ok(roles_pause_helpers::native_auth_error()))
    );
    constructor_helpers::assert_diagnostic(
        &env,
        soroban_sdk::xdr::ScError::WasmVm(ScErrorCode::InvalidAction),
    );
    assert_eq!(security_helpers::state(&env, &fixture.client.address), before);
    assert!(env.events().all().events().is_empty());
}

#[test]
#[should_panic(expected = "Error(Contract, #6309)")]
fn claim_rejects_a_negative_fee_from_a_project() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, -1);

    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 10, 19)]);
}
