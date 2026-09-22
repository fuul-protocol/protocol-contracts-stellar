use crate::test::{constructor_helpers::Bootstrap, *};
use fuul_core::ProjectFees;

const MANAGER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_manager.wasm"
));
const FACTORY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_factory.wasm"
));
const PROJECT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_project.wasm"
));

#[test]
fn current_wasm_api_reads_and_updates_fresh_numeric_storage_values() {
    let env = Env::default();
    env.mock_all_auths();
    env.cost_estimate().budget().reset_unlimited();
    let input = Bootstrap::new(&env);
    let manager = env.register(MANAGER, input.args(&env));
    let client = FuulManagerClient::new(&env, &manager);
    let limit_key = (Symbol::new(&env, "CurrencyLimit"), input.accepted.clone());
    let fee_key = (Symbol::new(&env, "FeeExemption"), input.admin.clone());
    let claims_key = (Symbol::new(&env, "UserClaims"), input.admin.clone(), input.accepted.clone());
    let existing = CurrencyTokenLimit {
        claim_limit_per_cooldown: u(&env, 100),
        cumulative_claim_per_cooldown: u(&env, 7),
        claim_cooldown_period_started: 1,
    };
    env.as_contract(&manager, || {
        env.storage().persistent().set(&limit_key, &existing);
        env.storage().persistent().set(&fee_key, &true);
        env.storage().persistent().set(&claims_key, &u(&env, 11));
    });
    assert_eq!(client.currency_limits(&input.accepted), existing);
    assert!(client.no_claim_fee_addresses(&input.admin));
    assert_eq!(client.users_claims(&input.admin, &input.accepted), u(&env, 11));
    client.set_currency_token_limit(&input.admin, &input.accepted, &u(&env, 150));
    client.remove_no_claim_fee_address(&input.admin, &input.admin);
    env.as_contract(&manager, || {
        assert!(!env.storage().persistent().has(&fee_key));
        let stored: CurrencyTokenLimit = env.storage().persistent().get(&limit_key).unwrap();
        assert_eq!(
            stored,
            CurrencyTokenLimit { claim_limit_per_cooldown: u(&env, 150), ..existing }
        );
        assert_eq!(env.storage().persistent().get::<_, U256>(&claims_key), Some(u(&env, 11)));
    });
    client.add_no_claim_fee_address(&input.admin, &input.admin);
    env.as_contract(&manager, || {
        assert_eq!(env.storage().persistent().get::<_, bool>(&fee_key), Some(true))
    });

    let project_hash = env.deployer().upload_contract_wasm(PROJECT);
    let collector = Address::generate(&env);
    let factory =
        env.register(FACTORY, (input.admin.clone(), manager, collector.clone(), project_hash));
    let factory_client = FuulFactoryClient::new(&env, &factory);
    let project = factory_client.create_fuul_project(
        &input.admin,
        &String::from_str(&env, "ipfs://new"),
        &false,
    );
    let fees_key = (Symbol::new(&env, "ProjectFees"), project.clone());
    let fees = ProjectFees { native_user_claim_fee: 123, project_claim_fee: 456, remove_fee: 789 };
    env.as_contract(&factory, || env.storage().persistent().set(&fees_key, &fees));
    let information = factory_client.get_fees_information(&project);
    assert_eq!(information.fees, fees);
    assert_eq!(information.fee_collector, collector);
    factory_client.set_native_user_claim_fee(&input.admin, &project, &124);
    env.as_contract(&factory, || {
        assert_eq!(
            env.storage().persistent().get::<_, ProjectFees>(&fees_key).unwrap(),
            ProjectFees { native_user_claim_fee: 124, ..fees }
        );
    });

    let project_client = FuulProjectClient::new(&env, &project);
    let uri_key = (Symbol::new(&env, "ProjectUri"),);
    let proof = BytesN::from_array(&env, &[37; 32]);
    let proof_key = (Symbol::new(&env, "ClaimedProof"), proof.clone());
    let historical_uri = String::from_str(&env, "ipfs://existing");
    env.as_contract(&project, || {
        env.storage().instance().set(&uri_key, &historical_uri);
        env.storage().persistent().set(&proof_key, &true);
    });
    assert_eq!(project_client.project_info_uri(), historical_uri);
    assert!(project_client.claimed_proofs(&proof));
    let updated_uri = String::from_str(&env, "ipfs://updated");
    project_client.set_project_uri(&input.admin, &updated_uri);
    env.as_contract(&project, || {
        assert_eq!(env.storage().instance().get::<_, String>(&uri_key), Some(updated_uri));
        assert_eq!(env.storage().persistent().get::<_, bool>(&proof_key), Some(true));
    });
}
