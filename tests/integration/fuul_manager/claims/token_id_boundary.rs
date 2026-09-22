use crate::test::{
    constructor_helpers::assert_diagnostic, roles_pause_helpers as auth, security_helpers::state, *,
};

#[test]
fn manager_to_real_project_rejects_token_id_above_u32_and_preserves_proof_for_retry() {
    let env = auth::test_env();
    let f = claim_fixture(&env);
    let nft = env.register(MockNonFungible, ());
    f.client.add_currency_limit(&f.admin, &nft, &u(&env, 100));
    f.client.add_no_claim_fee_address(&f.admin, &f.caller);
    let factory = env.register(
        FuulFactory,
        (
            f.admin.clone(),
            f.client.address.clone(),
            f.admin.clone(),
            env.deployer().upload_contract_wasm(PROJECT_WASM),
        ),
    );
    let project = FuulFactoryClient::new(&env, &factory).create_fuul_project(
        &f.admin,
        &String::from_str(&env, "ipfs://token-id-boundary"),
        &false,
    );
    MockNonFungibleClient::new(&env, &nft).mint(&project, &u32::MAX);
    let mut check = claim_check(&env, &f, &project, 0, 114);
    check.currency = nft.clone();
    check.currency_type = TokenType::NonFungible;
    for invalid in [
        u(&env, i128::from(u32::MAX) + 1),
        U256::from_parts(&env, u64::MAX, u64::MAX, u64::MAX, u64::MAX),
    ] {
        check.token_id = invalid;
        let before = state(&env, &f.client.address);
        let project_before = state(&env, &project);
        let nft_before = state(&env, &nft);
        assert_eq!(
            f.client.try_claim(&f.caller, &vec![&env, check.clone()]),
            Err(Ok(Error::from_contract_error(6105)))
        );
        assert_diagnostic(&env, soroban_sdk::xdr::ScError::Contract(6105));
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
        assert_eq!(state(&env, &project), project_before);
        assert_eq!(state(&env, &nft), nft_before);
        assert!(!FuulProjectClient::new(&env, &project).claimed_proofs(&check.proof));
    }
    check.token_id = u(&env, u32::MAX);
    f.client.claim(&f.caller, &vec![&env, check.clone()]);
    assert!(FuulProjectClient::new(&env, &project).claimed_proofs(&check.proof));
    assert_eq!(MockNonFungibleClient::new(&env, &nft).owner_of(&u32::MAX), f.recipient);
}
