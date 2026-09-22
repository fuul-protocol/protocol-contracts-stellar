use crate::test::*;

#[test]
fn admin_removes_stellar_assets_with_exact_auth_and_event() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 0);
    let receiver = Address::generate(&env);
    let token_ids = vec![&env, 7_i128];
    let amounts = Vec::new(&env);

    fixture.client.remove_funds(
        &fixture.admin,
        &receiver,
        &fixture.currency,
        &TokenType::StellarAsset,
        &100_000,
        &token_ids,
        &amounts,
    );
    let auths = env.auths();
    let events = env.events().all();

    let token = TokenClient::new(&env, &fixture.currency);
    assert_eq!(token.balance(&receiver), 100_000);
    assert_eq!(token.balance(&fixture.collector), 0);
    assert_eq!(token.balance(&fixture.client.address), 900_000);
    assert_eq!(
        auths,
        std::vec![(
            fixture.admin.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.client.address.clone(),
                    Symbol::new(&env, "remove_funds"),
                    (
                        fixture.admin.clone(),
                        receiver.clone(),
                        fixture.currency.clone(),
                        TokenType::StellarAsset,
                        100_000_i128,
                        token_ids.clone(),
                        amounts.clone(),
                    )
                        .into_val(&env),
                )),
                sub_invocations: std::vec![],
            },
        )]
    );
    assert_eq!(
        events.filter_by_contract(&fixture.client.address),
        std::vec![FundsRemoved {
            receiver,
            currency: fixture.currency,
            amount: 100_000,
            currency_type: TokenType::StellarAsset,
            token_ids,
            amounts,
        }
        .to_xdr(&env, &fixture.client.address)]
    );
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn removal_requires_project_admin_authorization() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 0);
    env.set_auths(&[]);

    fixture.client.remove_funds(
        &fixture.admin,
        &Address::generate(&env),
        &fixture.currency,
        &TokenType::StellarAsset,
        &100_000,
        &Vec::new(&env),
        &Vec::new(&env),
    );
}
