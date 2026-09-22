use crate::test::*;

#[test]
fn factory_manager_project_and_stellar_assets_complete_a_real_claim() {
    let env = Env::default();
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000_000);
    let signer = Address::generate(&env);
    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);
    let project_admin = Address::generate(&env);
    let factory_admin = Address::generate(&env);
    let collector = Address::generate(&env);
    let currency = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    let native_asset = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    let (manager, _, _, _) =
        register_manager(&env, 1, vec![&env, signer.clone()], &currency, &native_asset, None);
    let project_wasm_hash = env.deployer().upload_contract_wasm(PROJECT_WASM);
    let factory_address = env.register(
        FuulFactory,
        (factory_admin, manager.address.clone(), collector.clone(), project_wasm_hash),
    );
    let factory = FuulFactoryClient::new(&env, &factory_address);
    let project_address = factory.create_fuul_project(
        &project_admin,
        &soroban_sdk::String::from_str(&env, "ipfs://integrated-claim"),
        &false,
    );
    let project = FuulProjectClient::new(&env, &project_address);
    let claim_proof = BytesN::from_array(&env, &[23; 32]);
    let check = ClaimCheck {
        project_address: project_address.clone(),
        to: recipient.clone(),
        currency: currency.clone(),
        currency_type: TokenType::StellarAsset,
        amount: 100_000,
        reason: ClaimReason::EndUserPayout,
        token_id: u(&env, 0),
        deadline: u(&env, 1_000_300),
        proof: claim_proof.clone(),
        signers: vec![&env, signer],
    };
    env.mock_all_auths();
    StellarAssetClient::new(&env, &currency).mint(&project_address, &101_000);
    StellarAssetClient::new(&env, &native_asset).mint(&caller, &20_000);

    manager.claim(&caller, &vec![&env, check]);

    let currency_token = TokenClient::new(&env, &currency);
    let native_token = TokenClient::new(&env, &native_asset);
    assert_eq!(currency_token.balance(&recipient), 100_000);
    assert_eq!(currency_token.balance(&collector), 1_000);
    assert_eq!(currency_token.balance(&project_address), 0);
    assert_eq!(native_token.balance(&collector), 20_000);
    assert_eq!(native_token.balance(&caller), 0);
    assert_eq!(manager.users_claims(&recipient, &currency), u(&env, 100_000));
    assert!(project.claimed_proofs(&claim_proof));
}

#[test]
fn factory_manager_project_and_sep50_nft_complete_a_real_claim() {
    let env = Env::default();
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000_000);
    let signer = Address::generate(&env);
    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);
    let collector = Address::generate(&env);
    let currency = env.register(MockNonFungible, ());
    let native_asset = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    let (manager, _, _, _) =
        register_manager(&env, 1, vec![&env, signer.clone()], &currency, &native_asset, None);
    let project_wasm_hash = env.deployer().upload_contract_wasm(PROJECT_WASM);
    let factory_address = env.register(
        FuulFactory,
        (Address::generate(&env), manager.address.clone(), collector.clone(), project_wasm_hash),
    );
    let project_address = FuulFactoryClient::new(&env, &factory_address).create_fuul_project(
        &Address::generate(&env),
        &soroban_sdk::String::from_str(&env, "ipfs://integrated-sep50-claim"),
        &false,
    );
    let project = FuulProjectClient::new(&env, &project_address);
    let token = MockNonFungibleClient::new(&env, &currency);
    let claim_proof = BytesN::from_array(&env, &[24; 32]);
    env.mock_all_auths();
    token.mint(&project_address, &42);
    StellarAssetClient::new(&env, &native_asset).mint(&caller, &20_000);

    manager.claim(
        &caller,
        &vec![
            &env,
            ClaimCheck {
                project_address: project_address.clone(),
                to: recipient.clone(),
                currency: currency.clone(),
                currency_type: TokenType::NonFungible,
                amount: 0,
                reason: ClaimReason::EndUserPayout,
                token_id: u(&env, 42),
                deadline: u(&env, 1_000_300),
                proof: claim_proof.clone(),
                signers: vec![&env, signer],
            },
        ],
    );

    assert_eq!(
        env.events().all().filter_by_contract(&manager.address),
        std::vec![Claimed {
            project_address: project_address.clone(),
            to: recipient.clone(),
            currency: currency.clone(),
            amount: 0,
            currency_type: TokenType::NonFungible,
            token_id: u(&env, 42),
            reason: ClaimReason::EndUserPayout,
            proof: claim_proof.clone(),
        }
        .to_xdr(&env, &manager.address)]
    );
    assert_eq!(token.owner_of(&42), recipient.clone());
    assert_eq!(TokenClient::new(&env, &native_asset).balance(&collector), 20_000);
    assert_eq!(manager.users_claims(&recipient, &currency), u(&env, 0));
    assert!(project.claimed_proofs(&claim_proof));
}

#[test]
fn factory_manager_project_and_multi_token_adapter_complete_a_real_claim() {
    let env = Env::default();
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000_000);
    let signer = Address::generate(&env);
    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);
    let collector = Address::generate(&env);
    let currency = env.register(MockMultiToken, ());
    let native_asset = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    let (manager, _, _, _) =
        register_manager(&env, 1, vec![&env, signer.clone()], &currency, &native_asset, None);
    let project_wasm_hash = env.deployer().upload_contract_wasm(PROJECT_WASM);
    let factory_address = env.register(
        FuulFactory,
        (Address::generate(&env), manager.address.clone(), collector.clone(), project_wasm_hash),
    );
    let project_address = FuulFactoryClient::new(&env, &factory_address).create_fuul_project(
        &Address::generate(&env),
        &soroban_sdk::String::from_str(&env, "ipfs://integrated-multi-claim"),
        &false,
    );
    let project = FuulProjectClient::new(&env, &project_address);
    let token = MockMultiTokenClient::new(&env, &currency);
    let claim_proof = BytesN::from_array(&env, &[25; 32]);
    env.mock_all_auths();
    token.mint(&project_address, &7, &5);
    StellarAssetClient::new(&env, &native_asset).mint(&caller, &20_000);

    manager.claim(
        &caller,
        &vec![
            &env,
            ClaimCheck {
                project_address: project_address.clone(),
                to: recipient.clone(),
                currency: currency.clone(),
                currency_type: TokenType::MultiToken,
                amount: 0,
                reason: ClaimReason::EndUserPayout,
                token_id: u(&env, 7),
                deadline: u(&env, 1_000_300),
                proof: claim_proof.clone(),
                signers: vec![&env, signer],
            },
        ],
    );

    assert_eq!(
        env.events().all().filter_by_contract(&manager.address),
        std::vec![Claimed {
            project_address: project_address.clone(),
            to: recipient.clone(),
            currency: currency.clone(),
            amount: 0,
            currency_type: TokenType::MultiToken,
            token_id: u(&env, 7),
            reason: ClaimReason::EndUserPayout,
            proof: claim_proof.clone(),
        }
        .to_xdr(&env, &manager.address)]
    );
    assert_eq!(token.balance(&project_address, &7), 4);
    assert_eq!(token.balance(&recipient, &7), 1);
    assert_eq!(token.balance(&collector, &7), 0);
    assert_eq!(TokenClient::new(&env, &native_asset).balance(&collector), 20_000);
    assert_eq!(manager.users_claims(&recipient, &currency), u(&env, 0));
    assert!(project.claimed_proofs(&claim_proof));
}
