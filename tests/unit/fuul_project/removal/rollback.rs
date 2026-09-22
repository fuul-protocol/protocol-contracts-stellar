use crate::test::*;

#[test]
fn non_fungible_removal_reads_factory_fees_before_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let factory = env.register(FailingFactory, ());
    let project = env.register(
        FuulProject,
        (factory, admin.clone(), String::from_str(&env, "ipfs://fee-order"), false),
    );
    let receiver = Address::generate(&env);
    let currency = env.register(MockNonFungible, ());
    let token = MockNonFungibleClient::new(&env, &currency);
    token.mint(&project, &1);

    assert!(FuulProjectClient::new(&env, &project)
        .try_remove_funds(
            &admin,
            &receiver,
            &currency,
            &TokenType::NonFungible,
            &0,
            &vec![&env, 1_i128],
            &Vec::new(&env),
        )
        .is_err());
    assert_eq!(token.owner_of(&1), project);
}

#[test]
fn multi_token_removal_reads_factory_fees_before_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let factory = env.register(FailingFactory, ());
    let project = env.register(
        FuulProject,
        (factory, admin.clone(), String::from_str(&env, "ipfs://fee-order"), false),
    );
    let receiver = Address::generate(&env);
    let currency = env.register(MockMultiToken, ());
    let token = MockMultiTokenClient::new(&env, &currency);
    token.mint(&project, &7, &10);

    assert!(FuulProjectClient::new(&env, &project)
        .try_remove_funds(
            &admin,
            &receiver,
            &currency,
            &TokenType::MultiToken,
            &0,
            &vec![&env, 7_i128],
            &vec![&env, 10_i128],
        )
        .is_err());
    assert_eq!(token.balance(&project, &7), 10);
    assert_eq!(token.balance(&receiver, &7), 0);
}

#[test]
fn failed_multi_token_batch_rolls_back_earlier_transfers() {
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 0);
    let receiver = Address::generate(&env);
    let currency = env.register(MockMultiToken, ());
    let token = MockMultiTokenClient::new(&env, &currency);
    token.mint(&fixture.client.address, &1, &10);
    token.mint(&fixture.client.address, &2, &5);

    assert!(fixture
        .client
        .try_remove_funds(
            &fixture.admin,
            &receiver,
            &currency,
            &TokenType::MultiToken,
            &0,
            &vec![&env, 1_i128, 2_i128],
            &vec![&env, 10_i128, 6_i128],
        )
        .is_err());
    assert_eq!(token.balance(&fixture.client.address, &1), 10);
    assert_eq!(token.balance(&fixture.client.address, &2), 5);
    assert_eq!(token.balance(&receiver, &1), 0);
    assert_eq!(token.balance(&receiver, &2), 0);
}
