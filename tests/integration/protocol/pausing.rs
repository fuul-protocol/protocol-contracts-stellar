use crate::test::{roles_pause_helpers::*, *};

#[test]
fn paused_real_claim_preserves_funds_and_proof_then_succeeds_after_unpause() {
    let env = test_env();
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000_000);
    let signer = Address::generate(&env);
    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);
    let collector = Address::generate(&env);
    let currency = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    let native = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    let (manager, _, pauser, unpauser) =
        register_manager(&env, 1, vec![&env, signer.clone()], &currency, &native, None);
    let wasm_hash = env.deployer().upload_contract_wasm(PROJECT_WASM);
    let factory_id = env.register(
        FuulFactory,
        (Address::generate(&env), manager.address.clone(), collector.clone(), wasm_hash),
    );
    // Broad auth is confined to setup: real Factory deployment and SAC funding.
    env.mock_all_auths();
    let project_id = FuulFactoryClient::new(&env, &factory_id).create_fuul_project(
        &Address::generate(&env),
        &String::from_str(&env, "ipfs://pause-cycle"),
        &false,
    );
    let project = FuulProjectClient::new(&env, &project_id);
    StellarAssetClient::new(&env, &currency).mint(&project_id, &101_000);
    StellarAssetClient::new(&env, &native).mint(&caller, &20_000);
    env.set_auths(&[]);
    let proof = BytesN::from_array(&env, &[81; 32]);
    let check = ClaimCheck {
        project_address: project_id.clone(),
        to: recipient.clone(),
        currency: currency.clone(),
        currency_type: TokenType::StellarAsset,
        amount: 100_000,
        reason: ClaimReason::EndUserPayout,
        token_id: u(&env, 0),
        deadline: u(&env, 1_000_300),
        proof: proof.clone(),
        signers: vec![&env, signer.clone()],
    };
    let checks = vec![&env, check.clone()];
    let transfers = [MockAuthInvoke {
        contract: &native,
        fn_name: "transfer",
        args: (&caller, MuxedAddress::from(&collector), 20_000_i128).into_val(&env),
        sub_invokes: &[],
    }];
    let caller_invoke = MockAuthInvoke {
        contract: &manager.address,
        fn_name: "claim",
        args: (&caller, &checks).into_val(&env),
        sub_invokes: &transfers,
    };
    let signer_invoke = MockAuthInvoke {
        contract: &manager.address,
        fn_name: "claim",
        args: (check.authorization(),).into_val(&env),
        sub_invokes: &[],
    };
    let claim_auth = [
        MockAuth { address: &caller, invoke: &caller_invoke },
        MockAuth { address: &signer, invoke: &signer_invoke },
    ];
    let currency_token = TokenClient::new(&env, &currency);
    let native_token = TokenClient::new(&env, &native);
    let balances = || {
        [
            currency_token.balance(&project_id),
            currency_token.balance(&recipient),
            currency_token.balance(&collector),
            native_token.balance(&caller),
            native_token.balance(&collector),
        ]
    };
    assert_eq!(balances(), [101_000, 0, 0, 20_000, 0]);
    assert!(!project.claimed_proofs(&proof));
    assert_eq!(manager.users_claims(&recipient, &currency), u(&env, 0));
    let limit_before = manager.currency_limits(&currency);
    transition(&env, &manager, &pauser, true);
    env.mock_auths(&claim_auth);
    assert_eq!(manager.try_claim(&caller, &checks), Err(Ok(Error::from_contract_error(1000))));
    assert!(env.events().all().events().is_empty());
    assert!(manager.paused());
    assert_eq!(balances(), [101_000, 0, 0, 20_000, 0]);
    assert_eq!(manager.users_claims(&recipient, &currency), u(&env, 0));
    assert_eq!(manager.currency_limits(&currency), limit_before);
    assert!(!project.claimed_proofs(&proof));
    transition(&env, &manager, &unpauser, false);
    env.mock_auths(&claim_auth);
    manager.claim(&caller, &checks);
    assert_eq!(env.auths().len(), 2);
    assert_eq!(balances(), [0, 100_000, 1_000, 0, 20_000]);
    assert_eq!(manager.users_claims(&recipient, &currency), u(&env, 100_000));
    assert!(project.claimed_proofs(&proof));
    assert!(!manager.paused());
}
