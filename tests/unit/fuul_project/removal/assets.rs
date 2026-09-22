use crate::test::*;

#[test]
fn admin_removes_one_sep50_non_fungible_token_without_a_fee() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 500);
    let receiver = Address::generate(&env);
    let currency = env.register(MockNonFungible, ());
    let token = MockNonFungibleClient::new(&env, &currency);
    token.mint(&fixture.client.address, &1);

    fixture.client.remove_funds(
        &fixture.admin,
        &receiver,
        &currency,
        &TokenType::NonFungible,
        &0,
        &vec![&env, 1_i128],
        &Vec::new(&env),
    );

    assert_eq!(token.owner_of(&1), receiver);
}

#[test]
fn admin_removes_multiple_sep50_non_fungible_tokens() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 500);
    let receiver = Address::generate(&env);
    let currency = env.register(MockNonFungible, ());
    let token = MockNonFungibleClient::new(&env, &currency);
    for token_id in 1..=3 {
        token.mint(&fixture.client.address, &token_id);
    }
    let token_ids = vec![&env, 1_i128, 2_i128, 3_i128];

    fixture.client.remove_funds(
        &fixture.admin,
        &receiver,
        &currency,
        &TokenType::NonFungible,
        &0,
        &token_ids,
        &Vec::new(&env),
    );
    let project_events = env.events().all().filter_by_contract(&fixture.client.address);

    for token_id in 1..=3 {
        assert_eq!(token.owner_of(&token_id), receiver);
    }
    assert_eq!(
        project_events,
        std::vec![FundsRemoved {
            receiver,
            currency,
            amount: 0,
            currency_type: TokenType::NonFungible,
            token_ids,
            amounts: Vec::new(&env),
        }
        .to_xdr(&env, &fixture.client.address)]
    );
}

#[test]
fn admin_removes_one_multi_token_balance_without_a_fee() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 500);
    let receiver = Address::generate(&env);
    let currency = env.register(MockMultiToken, ());
    let token = MockMultiTokenClient::new(&env, &currency);
    token.mint(&fixture.client.address, &7, &10);

    fixture.client.remove_funds(
        &fixture.admin,
        &receiver,
        &currency,
        &TokenType::MultiToken,
        &0,
        &vec![&env, 7_i128],
        &vec![&env, 10_i128],
    );

    assert_eq!(token.balance(&fixture.client.address, &7), 0);
    assert_eq!(token.balance(&receiver, &7), 10);
    assert_eq!(token.balance(&fixture.collector, &7), 0);
}

#[test]
fn admin_removes_multiple_multi_token_balances() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 500);
    let receiver = Address::generate(&env);
    let currency = env.register(MockMultiToken, ());
    let token = MockMultiTokenClient::new(&env, &currency);
    let token_ids = vec![&env, 1_i128, 2_i128, 3_i128];
    let amounts = vec![&env, 10_i128, 20_i128, 30_i128];
    for index in 0..token_ids.len() {
        token.mint(
            &fixture.client.address,
            &(token_ids.get_unchecked(index) as u32),
            &amounts.get_unchecked(index),
        );
    }

    fixture.client.remove_funds(
        &fixture.admin,
        &receiver,
        &currency,
        &TokenType::MultiToken,
        &0,
        &token_ids,
        &amounts,
    );

    for index in 0..token_ids.len() {
        let token_id = token_ids.get_unchecked(index) as u32;
        assert_eq!(token.balance(&fixture.client.address, &token_id), 0);
        assert_eq!(token.balance(&receiver, &token_id), amounts.get_unchecked(index));
    }
}
