use crate::test::*;

mod initialization;
mod roles;

#[test]
fn constructor_stores_defaults_and_admin() {
    let env = Env::default();
    let fixture = fixture(&env);
    let access = FuulAccessControlClient::new(&env, &fixture.client.address);

    assert!(access.has_role(&access.default_admin_role(), &fixture.admin));
    assert_eq!(fixture.client.project_wasm_hash(), fixture.project_wasm_hash);
    assert_eq!(fixture.client.fee_collector(), fixture.collector);
    assert_eq!(fixture.client.contract_tracker(), 0);
    assert_eq!(fixture.client.default_native_claim_fee(), 20_000);
    assert_eq!(fixture.client.default_project_claim_fee(), 100);
    assert_eq!(fixture.client.default_remove_fee(), 0);
}

#[test]
fn constructor_grants_the_manager_role() {
    let env = Env::default();
    let fixture = fixture(&env);

    assert!(fixture.client.has_manager_role(&fixture.manager));
    assert!(!fixture.client.has_manager_role(&Address::generate(&env)));
    assert_eq!(fixture.client.manager_role(), soroban_sdk::Symbol::new(&env, "manager"));
}

#[test]
fn constructor_keeps_the_pinned_project_wasm_available_for_deployment() {
    let env = Env::default();
    let fixture = fixture(&env);

    assert_ne!(fixture.project_wasm_hash, BytesN::from_array(&env, &[0; 32]));
    create_fuul_project(&fixture, &env, "ipfs://pinned-wasm");
}
