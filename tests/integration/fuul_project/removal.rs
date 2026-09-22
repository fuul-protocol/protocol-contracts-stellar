use crate::test::*;

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn maximum_removal_rejects_insufficient_token_balance() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 10_000);

    fixture.client.remove_funds(
        &fixture.admin,
        &Address::generate(&env),
        &fixture.currency,
        &TokenType::StellarAsset,
        &i128::MAX,
        &Vec::new(&env),
        &Vec::new(&env),
    );
}

#[test]
fn failed_removal_rolls_back_all_token_transfers() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 500);
    let receiver = Address::generate(&env);

    assert!(fixture
        .client
        .try_remove_funds(
            &fixture.admin,
            &receiver,
            &fixture.currency,
            &TokenType::StellarAsset,
            &1_000_001,
            &Vec::new(&env),
            &Vec::new(&env),
        )
        .is_err());
    let token = TokenClient::new(&env, &fixture.currency);
    assert_eq!(token.balance(&receiver), 0);
    assert_eq!(token.balance(&fixture.collector), 0);
    assert_eq!(token.balance(&fixture.client.address), 1_000_000);
}
