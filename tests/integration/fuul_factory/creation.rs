use crate::test::{creation_helpers::*, *};
use soroban_sdk::{symbol_short, Bytes, Error, IntoVal, Symbol};

pub(in crate::test) fn historical_project_initialization() {
    let env = authority::test_env();
    let fixture = fixture(&env);
    let uri = String::from_str(&env, "ipfs://factory-project");
    let project = fixture.client.create_fuul_project(&fixture.project_admin, &uri, &true);
    let project_client = FuulProjectClient::new(&env, &project);
    let project_access = FuulAccessControlClient::new(&env, &project);
    assert_eq!(project_client.factory(), fixture.client.address);
    assert_eq!(project_client.project_info_uri(), uri);
    assert!(project_client.kyc_required());
    assert!(project_access.has_role(&authority::role(&env), &fixture.project_admin));
    assert_eq!(fixture.client.contract_tracker(), 1);
    assert_eq!(
        fixture.client.project_fees(&project),
        ProjectFees { native_user_claim_fee: 20_000, project_claim_fee: 100, remove_fee: 0 }
    );
}

pub(in crate::test) fn historical_project_removal() {
    let env = authority::test_env();
    let fixture = fixture(&env);
    let project = create_fuul_project(&fixture, &env, "ipfs://removal-integration");
    let receiver = Address::generate(&env);
    let currency = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    env.mock_all_auths();
    StellarAssetClient::new(&env, &currency).mint(&project, &100_000);
    fixture.client.set_remove_fee(&fixture.admin, &project, &500);
    FuulProjectClient::new(&env, &project).remove_funds(
        &fixture.project_admin,
        &receiver,
        &currency,
        &TokenType::StellarAsset,
        &100_000,
        &Vec::new(&env),
        &Vec::new(&env),
    );
    let token = TokenClient::new(&env, &currency);
    assert_eq!(token.balance(&receiver), 95_000);
    assert_eq!(token.balance(&fixture.collector), 5_000);
    assert_eq!(token.balance(&project), 0);
}

fn assert_failed_creation(e: &Env, id: &Address, admin: &Address, expected: Option<u32>) {
    let child = predicted(e, id, 0);
    let before = authority::state(e, id);
    assert!(authority::state(e, &child).is_empty());
    let result = FuulFactoryClient::new(e, id).try_create_fuul_project(
        admin,
        &String::from_str(e, "ipfs://retry"),
        &false,
    );
    if let Some(error) = expected {
        // deploy_v2 wraps constructor errors; the diagnostic retains the cause.
        assert!(result.is_err());
        use soroban_sdk::xdr::{ContractEventBody, ScError, ScVal};
        let diagnostics = e.host().get_diagnostic_events().unwrap();
        assert!(
            diagnostics.0.iter().any(|event| {
                let ContractEventBody::V0(body) = &event.event.body;
                event.failed_call && body.topics.contains(&ScVal::Error(ScError::Contract(error)))
            }),
            "missing constructor error {error:?}: {diagnostics:?}"
        );
    } else {
        assert!(result.is_err());
    }
    assert_eq!(authority::state(e, id), before);
    assert!(authority::state(e, &child).is_empty());
    assert!(e.events().all().events().is_empty());
    e.as_contract(id, || {
        assert_eq!(
            e.storage().instance().get::<_, u128>(&(Symbol::new(e, "ContractTracker"),)),
            Some(0)
        );
        assert!(!e.storage().persistent().has(&(Symbol::new(e, "ProjectFees"), child)));
    });
}

#[test]
fn missing_code_rolls_back_and_upload_repairs_the_same_salt() {
    for compiled in [false, true] {
        let e = authority::test_env();
        let admin = Address::generate(&e);
        let code: BytesN<32> = e.crypto().sha256(&Bytes::from_slice(&e, PROJECT_WASM)).into();
        let id = register(&e, compiled, &code, &admin);
        assert_failed_creation(&e, &id, &admin, None);
        assert_eq!(e.deployer().upload_contract_wasm(PROJECT_WASM), code);
        let client = FuulFactoryClient::new(&e, &id);
        let child =
            client.create_fuul_project(&admin, &String::from_str(&e, "ipfs://retry"), &false);
        assert_eq!(child, predicted(&e, &id, 0));
        assert_eq!(client.contract_tracker(), 1);
        assert_eq!(FuulProjectClient::new(&e, &child).factory(), id);
        assert_eq!(client.project_fees(&child).native_user_claim_fee, 20_000);
    }
}

#[test]
fn incompatible_constructor_and_unknown_hash_do_not_commit_child_or_fees() {
    for compiled in [false, true] {
        for unknown in [false, true] {
            let e = authority::test_env();
            let admin = Address::generate(&e);
            // Factory's third/fourth constructor inputs cannot accept Project's URI/bool.
            let code = if unknown {
                BytesN::from_array(&e, &[0; 32])
            } else {
                e.deployer().upload_contract_wasm(FACTORY_WASM)
            };
            let id = register(&e, compiled, &code, &admin);
            assert_failed_creation(&e, &id, &admin, None);
        }
    }
}

#[test]
fn late_constructor_panic_reverts_child_writes_events_and_allows_same_salt_retry() {
    for compiled in [false, true] {
        let e = authority::test_env();
        let gate = e.register(Gate, ());
        let code = e.deployer().upload_contract_wasm(PROBE_WASM);
        let id = register(&e, compiled, &code, &gate);
        assert_failed_creation(&e, &id, &gate, Some(8200));
        GateClient::new(&e, &gate).repair();
        let client = FuulFactoryClient::new(&e, &id);
        let child =
            client.create_fuul_project(&gate, &String::from_str(&e, "ipfs://retry"), &false);
        assert_eq!(child, predicted(&e, &id, 0));
        assert_eq!(client.contract_tracker(), 1);
        assert!(!authority::state(&e, &child).is_empty());
        e.as_contract(&child, || assert!(e.storage().instance().has(&symbol_short!("probe"))));
    }
}

#[test]
fn repeated_identical_inputs_use_unique_current_tracker_salts_without_admin_cosign() {
    let e = authority::test_env();
    let admin = Address::generate(&e);
    let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
    let id = register(&e, true, &code, &admin);
    let client = FuulFactoryClient::new(&e, &id);
    let uri = String::from_str(&e, "ipfs://same-inputs");
    for tracker in 0..3 {
        let child = client.create_fuul_project(&admin, &uri, &true);
        assert_eq!(child, predicted(&e, &id, tracker));
        assert!(e.auths().is_empty());
        let p = FuulProjectClient::new(&e, &child);
        assert_eq!(p.project_info_uri(), uri);
        assert!(p.kyc_required());
        assert!(FuulAccessControlClient::new(&e, &child).has_role(&authority::role(&e), &admin));
    }
}

#[test]
fn guest_rejects_corrupt_uint96_and_legacy_u64_storage_without_resetting_it() {
    for legacy in [false, true] {
        let e = authority::test_env();
        let admin = Address::generate(&e);
        let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
        let id = register(&e, true, &code, &admin);
        e.as_contract(&id, || {
            let key = (Symbol::new(&e, "ContractTracker"),);
            if legacy {
                e.storage().instance().set(&key, &0_u64);
            } else {
                e.storage().instance().set(&key, &(1_u128 << 96));
            }
        });
        let before = authority::state(&e, &id);
        let result = e.try_invoke_contract::<Address, Error>(
            &id,
            &Symbol::new(&e, "create_fuul_project"),
            (&admin, String::from_str(&e, "ipfs://invalid-state"), false).into_val(&e),
        );
        if legacy {
            assert!(result.is_err());
        } else {
            assert_eq!(result, Err(Ok(Error::from_contract_error(6202))));
        }
        assert_eq!(authority::state(&e, &id), before);
        assert!(e.events().all().events().is_empty());
    }
}

#[test]
fn guest_counter_does_not_stop_at_u64_limit() {
    let e = authority::test_env();
    let admin = Address::generate(&e);
    let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
    let id = register(&e, true, &code, &admin);
    assert_crosses_u64(&e, &id, &admin);
}

#[test]
fn guest_tracker_uses_uint96_domain_and_wraps_after_current_salt() {
    let e = authority::test_env();
    let admin = Address::generate(&e);
    let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
    let id = register(&e, true, &code, &admin);
    assert_domain(&e, &id, &admin);
}
