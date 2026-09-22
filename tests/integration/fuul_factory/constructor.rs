use crate::test::{authority::*, *};
use soroban_sdk::{contract, contractimpl, vec, Error, IntoVal, Symbol, U256};

const FACTORY_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_factory.wasm"
));

#[contract]
struct Deployer;

#[contractimpl]
impl Deployer {
    pub fn deploy(
        e: Env,
        code: BytesN<32>,
        project_code: BytesN<32>,
        admin: Address,
        manager: Address,
        collector: Address,
    ) -> Address {
        e.deployer()
            .with_current_contract(BytesN::from_array(&e, &[91; 32]))
            .deploy_v2(code, (admin, manager, collector, project_code))
    }
}

#[test]
fn guest_factory_bootstraps_explicit_admin_and_permissionless_project_initialization() {
    let e = test_env();
    let f = fixture(&e);
    let deployer = e.register(Deployer, ());
    let code = e.deployer().upload_contract_wasm(FACTORY_WASM);
    let id = DeployerClient::new(&e, &deployer).deploy(
        &code,
        &f.project_wasm_hash,
        &f.admin,
        &f.manager,
        &f.collector,
    );
    let client = FuulFactoryClient::new(&e, &id);
    let access = FuulAccessControlClient::new(&e, &id);
    assert_eq!(access.get_role_members(&role(&e)), vec![&e, f.admin.clone()]);
    assert!(!access.has_role(&role(&e), &deployer));
    assert_eq!(
        (
            client.default_native_claim_fee(),
            client.default_project_claim_fee(),
            client.default_remove_fee()
        ),
        (20_000, 100, 0)
    );
    let project = client.create_fuul_project(
        &f.project_admin,
        &String::from_str(&e, "ipfs://guest-constructor"),
        &true,
    );
    let project_events = e.events().all().filter_by_contract(&project);
    assert!(e.auths().is_empty());
    assert_eq!(FuulProjectClient::new(&e, &project).factory(), id.clone());
    assert!(FuulAccessControlClient::new(&e, &project).has_role(&role(&e), &f.project_admin));
    assert!(!FuulAccessControlClient::new(&e, &project).has_role(&role(&e), &f.admin));
    assert_eq!(project_events.events().len(), 3);
    assert_eq!(
        project_events.events()[0],
        stellar_access::access_control::RoleGranted {
            role: role(&e),
            account: f.project_admin.clone(),
            caller: id.clone(),
        }
        .to_xdr(&e, &project)
    );
    assert!(FuulProjectClient::new(&e, &project).kyc_required());
    let b = Address::generate(&e);
    authorize(&e, &id, &f.admin, "grant_role", (role(&e), &b, &f.admin).into_val(&e));
    access.grant_role(&role(&e), &b, &f.admin);
    authorize(&e, &id, &b, "set_default_remove_fee", (&b, 1_u32).into_val(&e));
    client.set_default_remove_fee(&b, &1);
    assert_eq!(client.default_remove_fee(), 1);
    assert!(e
        .try_invoke_contract::<soroban_sdk::Val, Error>(
            &id,
            &Symbol::new(&e, "get_admin"),
            Vec::new(&e)
        )
        .is_err());
}

#[test]
fn factory_manager_role_changes_reach_existing_real_project() {
    let e = test_env();
    let f = fixture(&e);
    let a = FuulAccessControlClient::new(&e, &f.client.address);
    let id = create_fuul_project(&f, &e, "ipfs://manager-role");
    let p = FuulProjectClient::new(&e, &id);
    let currency = e.register_stellar_asset_contract_v2(Address::generate(&e)).address();
    let to = Address::generate(&e);
    let proof = BytesN::from_array(&e, &[51; 32]);
    e.mock_all_auths();
    StellarAssetClient::new(&e, &currency).mint(&id, &101);
    a.revoke_role(&f.client.manager_role(), &f.manager, &f.admin);
    assert!(!f.client.has_manager_role(&f.manager));
    let before = state(&e, &id);
    assert_eq!(
        p.try_claim(
            &f.manager,
            &to,
            &currency,
            &TokenType::StellarAsset,
            &100,
            &U256::from_u32(&e, 0),
            &proof,
            &false
        ),
        Err(Ok(Error::from_contract_error(6101)))
    );
    assert_eq!(state(&e, &id), before);
    assert!(e.events().all().events().is_empty());
    a.grant_role(&f.client.manager_role(), &f.manager, &f.admin);
    p.claim(
        &f.manager,
        &to,
        &currency,
        &TokenType::StellarAsset,
        &100,
        &U256::from_u32(&e, 0),
        &proof,
        &false,
    );
    assert_eq!(TokenClient::new(&e, &currency).balance(&to), 100);
    assert_eq!(TokenClient::new(&e, &currency).balance(&f.collector), 1);
    assert!(p.claimed_proofs(&proof));
}
