use crate::test::*;

#[test]
fn fee_exempt_caller_does_not_pay_the_native_claim_fee() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    fixture.client.add_no_claim_fee_address(&fixture.admin, &fixture.caller);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 20_000);

    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 10, 4)]);

    let native = TokenClient::new(&env, &fixture.native_asset);
    assert_eq!(native.balance(&fixture.caller), 1_000_000);
    assert_eq!(native.balance(&collector), 0);
}

#[test]
fn batch_routes_total_native_fee_to_the_last_collector() {
    for second_fee in [30_000, 0] {
        let env = Env::default();
        let fixture = claim_fixture(&env);
        let first_collector = Address::generate(&env);
        let second_collector = Address::generate(&env);
        let first_project = register_mock_project(&env, &first_collector, 10_000);
        let second_project = register_mock_project(&env, &second_collector, second_fee);
        let first = claim_check(&env, &fixture, &first_project.address, 10, 5);
        let second = claim_check(&env, &fixture, &second_project.address, 20, 6);
        let first_event = Claimed {
            project_address: first.project_address.clone(),
            to: first.to.clone(),
            currency: first.currency.clone(),
            amount: first.amount,
            currency_type: first.currency_type,
            token_id: first.token_id.clone(),
            reason: first.reason,
            proof: first.proof.clone(),
        };
        let second_event = Claimed {
            project_address: second.project_address.clone(),
            to: second.to.clone(),
            currency: second.currency.clone(),
            amount: second.amount,
            currency_type: second.currency_type,
            token_id: second.token_id.clone(),
            reason: second.reason,
            proof: second.proof.clone(),
        };

        fixture.client.claim(&fixture.caller, &vec![&env, first, second]);
        let events = env.events().all();
        let caller_auth =
            env.auths().into_iter().find(|(account, _)| account == &fixture.caller).unwrap().1;

        let native = TokenClient::new(&env, &fixture.native_asset);
        assert_eq!(native.balance(&first_collector), 0);
        assert_eq!(native.balance(&second_collector), 10_000 + second_fee);
        assert_eq!(native.balance(&fixture.caller), 990_000 - second_fee);
        assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 30));
        assert_eq!(first_project.last_recipient(), Some(fixture.recipient.clone()));
        assert_eq!(second_project.last_recipient(), Some(fixture.recipient.clone()));

        assert_eq!(events.events().len(), 3);
        assert_eq!(
            &events.events()[..2],
            &[
                first_event.to_xdr(&env, &fixture.client.address),
                second_event.to_xdr(&env, &fixture.client.address),
            ]
        );
        assert_eq!(events.filter_by_contract(&fixture.native_asset).events().len(), 1);
        assert_eq!(
            caller_auth.sub_invocations,
            std::vec![AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.native_asset.clone(),
                    symbol_short!("transfer"),
                    (&fixture.caller, MuxedAddress::from(&second_collector), 10_000 + second_fee)
                        .into_val(&env)
                )),
                sub_invocations: std::vec![],
            }]
        );
    }
}

#[test]
fn insufficient_native_fee_balance_rolls_back_the_complete_batch() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let first_collector = Address::generate(&env);
    let second_collector = Address::generate(&env);
    let first_project = register_mock_project(&env, &first_collector, 600_000);
    let second_project = register_mock_project(&env, &second_collector, 600_000);
    let first = claim_check(&env, &fixture, &first_project.address, 10, 57);
    let second = claim_check(&env, &fixture, &second_project.address, 20, 58);

    assert_eq!(
        fixture.client.try_claim(&fixture.caller, &vec![&env, first, second]),
        Err(Ok(Error::from_contract_error(10)))
    );

    let native = TokenClient::new(&env, &fixture.native_asset);
    assert_eq!(native.balance(&fixture.caller), 1_000_000);
    assert_eq!(native.balance(&first_collector), 0);
    assert_eq!(native.balance(&second_collector), 0);
    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 0));
    assert_eq!(
        fixture.client.currency_limits(&fixture.currency).cumulative_claim_per_cooldown,
        u(&env, 0)
    );
    assert_eq!(first_project.last_recipient(), None);
    assert_eq!(second_project.last_recipient(), None);
    assert!(env.events().all().events().is_empty());
}

#[test]
fn zero_exempt_and_empty_batches_do_not_authorize_a_fee_transfer() {
    for mode in 0..3 {
        let env = Env::default();
        let f = claim_fixture(&env);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let fee = if mode == 1 { i128::MAX } else { 0 };
        let first = register_mock_project(&env, &a, fee);
        let second = register_mock_project(&env, &b, fee);
        if mode == 1 {
            f.client.add_no_claim_fee_address(&f.admin, &f.caller);
        }
        let checks = if mode == 2 {
            Vec::new(&env)
        } else {
            vec![
                &env,
                claim_check(&env, &f, &first.address, 10, 91),
                claim_check(&env, &f, &second.address, 20, 92),
            ]
        };
        security_helpers::authorize_claims(&env, &f.client.address, &f.caller, &checks, &[]);
        f.client.claim(&f.caller, &checks);
        let events = env.events().all();
        assert_eq!(events.filter_by_contract(&f.native_asset).events().len(), 0);
        assert_eq!(
            events.filter_by_contract(&f.client.address).events().len(),
            if mode == 2 { 0 } else { 2 }
        );
        let caller_auth =
            env.auths().into_iter().find(|(account, _)| account == &f.caller).unwrap().1;
        assert!(caller_auth.sub_invocations.is_empty());
        assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&f.caller), 1_000_000);
        assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&a), 0);
        assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&b), 0);
    }
}

#[test]
fn aggregate_fee_overflow_rolls_back_before_any_native_payment() {
    let env = Env::default();
    let f = claim_fixture(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let first = register_mock_project(&env, &a, i128::MAX);
    let second = register_mock_project(&env, &b, 1);
    let checks = vec![
        &env,
        claim_check(&env, &f, &first.address, 10, 93),
        claim_check(&env, &f, &second.address, 20, 94),
    ];
    let before = security_helpers::state(&env, &f.client.address);
    let first_before = security_helpers::state(&env, &first.address);
    let second_before = security_helpers::state(&env, &second.address);
    assert_eq!(f.client.try_claim(&f.caller, &checks), Err(Ok(Error::from_contract_error(6308))));
    assert!(env.events().all().events().is_empty());
    assert_eq!(security_helpers::state(&env, &f.client.address), before);
    assert_eq!(security_helpers::state(&env, &first.address), first_before);
    assert_eq!(security_helpers::state(&env, &second.address), second_before);
    assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&f.caller), 1_000_000);
    assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&a), 0);
    assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&b), 0);
}

#[test]
fn aggregate_native_fee_accepts_i128_max_exactly_once() {
    let env = Env::default();
    let f = claim_fixture(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let first = register_mock_project(&env, &a, i128::MAX - 1);
    let second = register_mock_project(&env, &b, 1);
    StellarAssetClient::new(&env, &f.native_asset).mint(&f.caller, &(i128::MAX - 1_000_000));
    f.client.claim(
        &f.caller,
        &vec![
            &env,
            claim_check(&env, &f, &first.address, 10, 95),
            claim_check(&env, &f, &second.address, 20, 96),
        ],
    );
    assert_eq!(env.events().all().filter_by_contract(&f.native_asset).events().len(), 1);
    assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&f.caller), 0);
    assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&a), 0);
    assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&b), i128::MAX);
}

#[test]
fn removing_a_fee_exemption_restores_the_native_fee() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 20_000);
    let native = TokenClient::new(&env, &fixture.native_asset);
    fixture.client.add_no_claim_fee_address(&fixture.admin, &fixture.caller);

    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 10, 34)]);
    assert_eq!(native.balance(&collector), 0);

    fixture.client.remove_no_claim_fee_address(&fixture.admin, &fixture.caller);
    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 10, 35)]);
    assert_eq!(native.balance(&collector), 20_000);
}
