use crate::test::{
    constructor_helpers::*,
    roles_pause_helpers::{test_env, transition},
    *,
};
use soroban_sdk::{testutils::Deployer, xdr::ScError};

// A test-only transaction frame around the official deployment API. It deliberately
// models no production deployer policy or account-signature authorization.
#[contract]
struct ConstructorDeployer;
#[contractimpl]
impl ConstructorDeployer {
    pub fn deploy(env: Env, wasm: BytesN<32>, salt: BytesN<32>, args: Vec<Val>) -> Address {
        env.deployer().with_current_contract(salt).deploy_v2(wasm, args)
    }
}

fn instance_exists(env: &Env, id: &Address) -> bool {
    let address: ScAddress = id.clone().into();
    env.to_ledger_snapshot().ledger_entries.iter().any(|(key, _)| {
        matches!(key.as_ref(), LedgerKey::ContractData(data) if data.contract == address)
    })
}

#[test]
fn source_quorum_five_and_nonadjacent_duplicate_signers() {
    for compiled in [false, true] {
        let e = test_env();
        for count in [1_u32, 5] {
            let mut input = Bootstrap::new(&e);
            input.quorum = u128::from(count);
            input.signers =
                (0..count).map(|_| Address::generate(&e)).fold(Vec::new(&e), |mut v, s| {
                    v.push_back(s);
                    v
                });
            let id = if compiled {
                e.register(MANAGER_WASM, input.args(&e))
            } else {
                input.register(&e)
            };
            let c = FuulManagerClient::new(&e, &id);
            let access = FuulAccessControlClient::new(&e, &id);
            assert_eq!(c.required_signers(), u128::from(count));
            assert_eq!(access.get_role_members(&c.claim_signer_role()), input.signers);
        }
        let e = test_env();
        let mut input = Bootstrap::new(&e);
        let a = input.signers.get(0).unwrap();
        input.signers = vec![&e, a.clone(), Address::generate(&e), a];
        input.quorum = 3;
        if compiled {
            let wasm = e.deployer().upload_contract_wasm(MANAGER_WASM);
            let deployer_id = e.register(ConstructorDeployer, ());
            let salt = BytesN::from_array(&e, &[94; 32]);
            let id =
                e.deployer().with_address(deployer_id.clone(), salt.clone()).deployed_address();
            let before = e.to_ledger_snapshot().ledger_entries;
            assert_eq!(
                ConstructorDeployerClient::new(&e, &deployer_id).try_deploy(
                    &wasm,
                    &salt,
                    &input.args(&e)
                ),
                Err(Ok(Error::from_type_and_code(
                    ScErrorType::Context,
                    ScErrorCode::InvalidAction
                )))
            );
            assert_diagnostic(&e, ScError::Contract(6301));
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(!instance_exists(&e, &id));
            assert!(e.events().all().events().is_empty());
        } else {
            reject_native(&e, &input, 6301);
        }
    }
}

#[test]
fn wasm_bootstrap_uses_explicit_admin_and_rejects_same_address_redeployment() {
    let env = test_env();
    let mut input = Bootstrap::new(&env);
    input.accepted = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    input.native = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    let wasm = env.deployer().upload_contract_wasm(MANAGER_WASM);
    let deployer_id = env.register(ConstructorDeployer, ());
    assert_ne!(input.admin, deployer_id);
    let deployer = ConstructorDeployerClient::new(&env, &deployer_id);
    let salt = BytesN::from_array(&env, &[91; 32]);
    let args = input.args(&env);
    let id = deployer.deploy(&wasm, &salt, &args);
    assert_eq!(env.events().all().filter_by_contract(&id), expected_events(&env, &id, &input));
    assert_eq!(env.deployer().get_contract_code_ttl(&id), fuul_core::INSTANCE_EXTEND_AMOUNT);
    assert_bootstrap(&env, &id, &input);
    let client = FuulManagerClient::new(&env, &id);
    let access = FuulAccessControlClient::new(&env, &id);
    assert_eq!(
        access.get_role_members(&access.default_admin_role()),
        vec![&env, input.admin.clone()]
    );
    let mut replacement = input.clone();
    replacement.admin = Address::generate(&env);
    replacement.quorum = 0;
    let result = deployer.try_deploy(&wasm, &salt, &replacement.args(&env));
    assert_eq!(
        result,
        Err(Ok(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)))
    );
    assert_diagnostic(&env, ScError::Storage(ScErrorCode::ExistingValue));
    assert!(env.events().all().events().is_empty());
    assert_bootstrap(&env, &id, &input);
    transition(&env, &client, &input.pauser, true);
    assert!(client.paused());
    transition(&env, &client, &input.unpauser, false);
    assert!(!client.paused());
}

#[test]
fn failed_wasm_constructor_is_atomic_and_same_salt_can_retry() {
    let env = test_env();
    let mut input = Bootstrap::new(&env);
    input.accepted = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    input.native = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    let wasm = env.deployer().upload_contract_wasm(MANAGER_WASM);
    let deployer_id = env.register(ConstructorDeployer, ());
    let deployer = ConstructorDeployerClient::new(&env, &deployer_id);
    let salt = BytesN::from_array(&env, &[92; 32]);
    let expected_id = env.deployer().with_address(deployer_id, salt.clone()).deployed_address();
    assert!(!instance_exists(&env, &expected_id));
    let mut invalid = input.clone();
    invalid.signers.push_back(invalid.signers.get(0).unwrap());
    assert_eq!(
        deployer.try_deploy(&wasm, &salt, &invalid.args(&env)),
        Err(Ok(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)))
    );
    assert_diagnostic(&env, ScError::Context(ScErrorCode::InvalidAction));
    assert_diagnostic(&env, ScError::Contract(6301));
    assert!(env.events().all().events().is_empty());
    assert!(!instance_exists(&env, &expected_id));
    for quorum in [1_u128 << 96, u128::MAX] {
        let mut oversized = input.clone();
        oversized.quorum = quorum;
        let before = env.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            deployer.try_deploy(&wasm, &salt, &oversized.args(&env)),
            Err(Ok(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)))
        );
        assert_diagnostic(&env, ScError::Contract(6300));
        assert!(env.events().all().events().is_empty());
        assert_eq!(env.to_ledger_snapshot().ledger_entries, before);
        assert!(!instance_exists(&env, &expected_id));
    }
    let id = deployer.deploy(&wasm, &salt, &input.args(&env));
    assert_eq!(id, expected_id);
    assert!(instance_exists(&env, &id));
    assert_bootstrap(&env, &id, &input);
}

#[test]
fn nonpositive_initial_currency_limit_rolls_back_wasm_deployment_and_retries() {
    for limit in [0_i128, -1, i128::MIN] {
        let env = test_env();
        let input = Bootstrap::new(&env);
        let wasm = env.deployer().upload_contract_wasm(MANAGER_WASM);
        let deployer_id = env.register(ConstructorDeployer, ());
        let deployer = ConstructorDeployerClient::new(&env, &deployer_id);
        let salt = BytesN::from_array(&env, &[93; 32]);
        let id = env.deployer().with_address(deployer_id, salt.clone()).deployed_address();
        let mut args = input.args(&env);
        args.set(8, if limit == 0 { u(&env, 0).into_val(&env) } else { limit.into_val(&env) });
        let before = env.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            deployer.try_deploy(&wasm, &salt, &args),
            Err(Ok(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)))
        );
        assert_diagnostic(
            &env,
            if limit == 0 {
                ScError::Contract(6300)
            } else {
                ScError::WasmVm(ScErrorCode::InvalidAction)
            },
        );
        assert_eq!(env.to_ledger_snapshot().ledger_entries, before);
        assert!(env.events().all().events().is_empty());
        assert!(!instance_exists(&env, &id));
        args.set(8, u(&env, 1).into_val(&env));
        assert_eq!(deployer.deploy(&wasm, &salt, &args), id);
        let manager = FuulManagerClient::new(&env, &id);
        assert_eq!(manager.currency_limits(&input.accepted).claim_limit_per_cooldown, u(&env, 1));
        assert_eq!(
            manager.currency_limits(&input.native).claim_limit_per_cooldown,
            u(&env, 1_000_000_000_000_i128)
        );
    }
}
