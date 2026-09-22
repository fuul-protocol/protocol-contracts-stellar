use crate::test::*;

#[test]
fn removal_accepts_zero_stellar_asset_amount() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 0);
    let receiver = Address::generate(&env);

    fixture.client.remove_funds(
        &fixture.admin,
        &receiver,
        &fixture.currency,
        &TokenType::StellarAsset,
        &0,
        &Vec::new(&env),
        &Vec::new(&env),
    );
    let events = env.events().all().filter_by_contract(&fixture.client.address);

    let token = TokenClient::new(&env, &fixture.currency);
    assert_eq!(token.balance(&receiver), 0);
    assert_eq!(token.balance(&fixture.collector), 0);
    assert_eq!(token.balance(&fixture.client.address), 1_000_000);
    assert_eq!(
        events,
        std::vec![FundsRemoved {
            receiver,
            currency: fixture.currency,
            amount: 0,
            currency_type: TokenType::StellarAsset,
            token_ids: Vec::new(&env),
            amounts: Vec::new(&env),
        }
        .to_xdr(&env, &fixture.client.address)]
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6105)")]
fn removal_rejects_negative_soroban_amount() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 0);

    fixture.client.remove_funds(
        &fixture.admin,
        &Address::generate(&env),
        &fixture.currency,
        &TokenType::StellarAsset,
        &-1,
        &Vec::new(&env),
        &Vec::new(&env),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6105)")]
fn multi_token_removal_rejects_mismatched_arrays() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 0);
    let currency = env.register(MockMultiToken, ());

    fixture.client.remove_funds(
        &fixture.admin,
        &Address::generate(&env),
        &currency,
        &TokenType::MultiToken,
        &0,
        &vec![&env, 1_i128, 2_i128],
        &vec![&env, 10_i128],
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6105)")]
fn multi_token_removal_rejects_negative_balances() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 0);
    let currency = env.register(MockMultiToken, ());

    fixture.client.remove_funds(
        &fixture.admin,
        &Address::generate(&env),
        &currency,
        &TokenType::MultiToken,
        &0,
        &vec![&env, 1_i128],
        &vec![&env, -1_i128],
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6105)")]
fn non_fungible_removal_rejects_an_id_above_the_sep50_range() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 0);
    let currency = env.register(MockNonFungible, ());

    fixture.client.remove_funds(
        &fixture.admin,
        &Address::generate(&env),
        &currency,
        &TokenType::NonFungible,
        &0,
        &vec![&env, i128::from(u32::MAX) + 1],
        &Vec::new(&env),
    );
}
