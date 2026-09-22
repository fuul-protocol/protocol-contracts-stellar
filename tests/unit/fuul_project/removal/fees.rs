use crate::test::*;

#[test]
fn removal_fee_is_deducted_from_the_receiver_amount() {
    for (bps, expected_fee) in [
        (100, 1_000),
        (500, 5_000),
        (1_000, 10_000),
        (2_000, 20_000),
        (2_500, 25_000),
        (10_000, 100_000),
    ] {
        let env = Env::default();
        let fixture = project_fixture(&env, false, 0, 0, bps);
        let receiver = Address::generate(&env);

        fixture.client.remove_funds(
            &fixture.admin,
            &receiver,
            &fixture.currency,
            &TokenType::StellarAsset,
            &100_000,
            &Vec::new(&env),
            &Vec::new(&env),
        );

        let token = TokenClient::new(&env, &fixture.currency);
        assert_eq!(token.balance(&receiver), 100_000 - expected_fee);
        assert_eq!(token.balance(&fixture.collector), expected_fee);
        assert_eq!(token.balance(&fixture.client.address), 900_000);
    }
}

#[test]
fn repeated_removal_uses_the_same_configured_fee() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 500);
    let receiver = Address::generate(&env);

    for _ in 0..2 {
        fixture.client.remove_funds(
            &fixture.admin,
            &receiver,
            &fixture.currency,
            &TokenType::StellarAsset,
            &100_000,
            &Vec::new(&env),
            &Vec::new(&env),
        );
    }

    let token = TokenClient::new(&env, &fixture.currency);
    assert_eq!(token.balance(&receiver), 190_000);
    assert_eq!(token.balance(&fixture.collector), 10_000);
    assert_eq!(token.balance(&fixture.client.address), 800_000);
}

#[test]
fn one_hundred_percent_removal_fee_sends_the_amount_to_the_collector() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 10_000);
    let receiver = Address::generate(&env);

    fixture.client.remove_funds(
        &fixture.admin,
        &receiver,
        &fixture.currency,
        &TokenType::StellarAsset,
        &100_000,
        &Vec::new(&env),
        &Vec::new(&env),
    );

    let token = TokenClient::new(&env, &fixture.currency);
    assert_eq!(token.balance(&receiver), 0);
    assert_eq!(token.balance(&fixture.collector), 100_000);
}

#[test]
fn removal_accepts_a_configured_fee_that_rounds_to_zero() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 100);
    let receiver = Address::generate(&env);

    fixture.client.remove_funds(
        &fixture.admin,
        &receiver,
        &fixture.currency,
        &TokenType::StellarAsset,
        &99,
        &Vec::new(&env),
        &Vec::new(&env),
    );

    let token = TokenClient::new(&env, &fixture.currency);
    assert_eq!(token.balance(&receiver), 99);
    assert_eq!(token.balance(&fixture.collector), 0);
    assert_eq!(token.balance(&fixture.client.address), 999_901);
}
