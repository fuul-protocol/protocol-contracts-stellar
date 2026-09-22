use crate::test::*;
use soroban_sdk::{
    testutils::{MockAuth, MockAuthInvoke},
    Error, Val,
};

const PROJECT_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_project.wasm"
));

fn project(e: &Env, guest: bool) -> (Address, Address, Address) {
    e.cost_estimate().budget().reset_unlimited();
    e.ledger().with_mut(|l| l.min_persistent_entry_ttl = 90 * 17_280 + 1);
    let admin = Address::generate(e);
    let factory = e.register(
        MockFactory,
        (
            Address::generate(e),
            FeesInformation {
                fee_collector: Address::generate(e),
                fees: ProjectFees { native_user_claim_fee: 0, project_claim_fee: 0, remove_fee: 0 },
            },
        ),
    );
    let id = Address::generate(e);
    let args: Vec<Val> =
        (&factory, &admin, String::from_str(e, "ipfs://governance"), false).into_val(e);
    // Setup uses a contract Factory double; MockAuth would replace its dispatch.
    // Every governance invocation below supplies its own exact authorization.
    e.mock_all_auths();
    if guest {
        e.register_at(&id, PROJECT_WASM, args);
    } else {
        e.register_at(&id, FuulProject, args);
    }
    e.mock_auths(&[]);
    (id, admin, factory)
}

fn authorize(e: &Env, id: &Address, actor: &Address, name: &str, args: Vec<Val>) {
    e.mock_auths(&[MockAuth {
        address: actor,
        invoke: &MockAuthInvoke { contract: id, fn_name: name, args, sub_invokes: &[] },
    }]);
}

#[test]
fn default_admin_role_is_public() {
    let e = Env::default();
    let f = fixture(&e);
    assert_eq!(
        e.try_invoke_contract::<Symbol, Error>(
            &f.client.address,
            &Symbol::new(&e, "default_admin_role"),
            Vec::new(&e),
        ),
        Ok(Ok(Symbol::new(&e, "default_admin"))),
    );
}

#[test]
fn granted_administrator_can_update_project_without_the_initial_admin() {
    let e = Env::default();
    let f = fixture(&e);
    let b = Address::generate(&e);
    let role = Symbol::new(&e, "default_admin");
    let args: Vec<Val> = (&role, &b, &f.admin).into_val(&e);
    authorize(&e, &f.client.address, &f.admin, "grant_role", args.clone());
    e.invoke_contract::<()>(&f.client.address, &Symbol::new(&e, "grant_role"), args);
    assert!(e.as_contract(&f.client.address, || {
        stellar_access::access_control::has_role(&e, &b, &role).is_some()
    }));
    let uri = String::from_str(&e, "ipfs://second-admin");
    let args: Vec<Val> = (&b, &uri).into_val(&e);
    authorize(&e, &f.client.address, &b, "set_project_uri", args.clone());
    assert_eq!(
        e.try_invoke_contract::<(), Error>(
            &f.client.address,
            &Symbol::new(&e, "set_project_uri"),
            args,
        ),
        Ok(Ok(())),
        "a granted administrator must not need the initial administrator's authorization",
    );
    assert_eq!(f.client.project_info_uri(), uri);
}

#[test]
fn administrators_independently_mutate_and_revocation_takes_effect_in_native_and_wasm() {
    for guest in [false, true] {
        let e = Env::default();
        let (id, a, _) = project(&e, guest);
        let p = FuulProjectClient::new(&e, &id);
        let access = FuulAccessControlClient::new(&e, &id);
        let role = access.default_admin_role();
        let b = Address::generate(&e);
        authorize(&e, &id, &a, "grant_role", (&role, &b, &a).into_val(&e));
        access.grant_role(&role, &b, &a);
        let token = e.register_stellar_asset_contract_v2(a.clone()).address();
        authorize(&e, &token, &a, "mint", (&id, 100_i128).into_val(&e));
        StellarAssetClient::new(&e, &token).mint(&id, &100);
        let receiver = Address::generate(&e);
        for actor in [&a, &b] {
            let uri = actor.to_string();
            authorize(&e, &id, actor, "set_project_uri", (actor, &uri).into_val(&e));
            p.set_project_uri(actor, &uri);
            assert_eq!(p.project_info_uri(), uri);
            authorize(&e, &id, actor, "set_kyc_required", (actor, true).into_val(&e));
            p.set_kyc_required(actor, &true);
            assert!(p.kyc_required());
            let empty = Vec::<i128>::new(&e);
            authorize(
                &e,
                &id,
                actor,
                "remove_funds",
                (actor, &receiver, &token, TokenType::StellarAsset, 10_i128, &empty, &empty)
                    .into_val(&e),
            );
            p.remove_funds(actor, &receiver, &token, &TokenType::StellarAsset, &10, &empty, &empty);
        }
        assert_eq!(TokenClient::new(&e, &token).balance(&receiver), 20);
        assert_eq!(TokenClient::new(&e, &token).balance(&id), 80);
        authorize(&e, &id, &a, "revoke_role", (&role, &b, &a).into_val(&e));
        access.revoke_role(&role, &b, &a);
        let empty = Vec::<i128>::new(&e);
        let calls: [(&str, Vec<Val>); 3] = [
            ("set_project_uri", (&b, String::from_str(&e, "ipfs://revoked")).into_val(&e)),
            ("set_kyc_required", (&b, false).into_val(&e)),
            (
                "remove_funds",
                (&b, &receiver, &token, TokenType::StellarAsset, 1_i128, &empty, &empty)
                    .into_val(&e),
            ),
        ];
        for (name, args) in calls {
            authorize(&e, &id, &b, name, args.clone());
            let before = e.to_ledger_snapshot().ledger_entries;
            assert_eq!(
                e.try_invoke_contract::<(), Error>(&id, &Symbol::new(&e, name), args),
                Err(Ok(Error::from_contract_error(2000)))
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
        }
    }
}

#[test]
fn role_enumeration_no_ops_and_last_admin_renunciation_in_native_and_wasm() {
    for guest in [false, true] {
        let e = Env::default();
        let (id, a, _) = project(&e, guest);
        let access = FuulAccessControlClient::new(&e, &id);
        let role = access.default_admin_role();
        let b = Address::generate(&e);
        let absent = Symbol::new(&e, "unassigned");
        assert_eq!(access.get_role_admin(&absent), role);
        assert_eq!(access.get_role_member_count(&absent), 0);
        assert_eq!(access.get_role_members(&absent), Vec::<Address>::new(&e));
        assert!(!access.has_role(&absent, &a));
        assert_eq!(
            access.try_get_role_member(&absent, &0),
            Err(Ok(Error::from_contract_error(2002)))
        );
        authorize(&e, &id, &a, "grant_role", (&role, &b, &a).into_val(&e));
        access.grant_role(&role, &b, &a);
        assert_eq!(access.get_role_member_count(&role), 2);
        assert_eq!(access.get_role_members(&role), vec![&e, a.clone(), b.clone()]);
        assert_eq!(access.get_role_member(&role, &1), b);
        for (name, args) in [
            ("grant_role", (&role, &b, &a).into_val(&e)),
            ("revoke_role", (&absent, &b, &a).into_val(&e)),
            ("renounce_role", (&absent, &a).into_val(&e)),
        ] {
            let args: Vec<Val> = args;
            e.mock_auths(&[]);
            let before = e.to_ledger_snapshot().ledger_entries;
            assert!(e
                .try_invoke_contract::<(), Error>(&id, &Symbol::new(&e, name), args.clone())
                .is_err());
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
            authorize(&e, &id, &a, name, args.clone());
            e.invoke_contract::<()>(&id, &Symbol::new(&e, name), args);
            assert!(e.events().all().events().is_empty());
            assert_eq!(access.get_role_members(&role), vec![&e, a.clone(), b.clone()]);
        }
        authorize(&e, &id, &a, "renounce_role", (&role, &a).into_val(&e));
        access.renounce_role(&role, &a);
        assert_eq!(
            e.events().all().events(),
            &[stellar_access::access_control::RoleRevoked {
                role: role.clone(),
                account: a.clone(),
                caller: a.clone()
            }
            .to_xdr(&e, &id)]
        );
        assert_eq!(access.get_role_members(&role), vec![&e, b.clone()]);
        authorize(&e, &id, &b, "set_kyc_required", (&b, true).into_val(&e));
        FuulProjectClient::new(&e, &id).set_kyc_required(&b, &true);
        authorize(&e, &id, &b, "renounce_role", (&role, &b).into_val(&e));
        access.renounce_role(&role, &b);
        assert_eq!(access.get_role_member_count(&role), 0);
        assert!(!access.has_role(&role, &b));
        authorize(&e, &id, &b, "set_kyc_required", (&b, false).into_val(&e));
        assert_eq!(
            FuulProjectClient::new(&e, &id).try_set_kyc_required(&b, &false),
            Err(Ok(Error::from_contract_error(2000)))
        );
    }
}

#[test]
fn singleton_and_delegation_endpoints_are_absent_in_native_and_wasm() {
    for guest in [false, true] {
        let e = Env::default();
        let (id, a, _) = project(&e, guest);
        assert_eq!(e.as_contract(&id, || stellar_access::access_control::get_admin(&e)), None);
        for (name, args) in [
            ("get_admin", Vec::new(&e)),
            ("get_existing_roles", Vec::new(&e)),
            ("renounce_admin", Vec::new(&e)),
            ("accept_admin_transfer", Vec::new(&e)),
            ("transfer_admin_role", (&a, 100_u32).into_val(&e)),
            (
                "set_role_admin",
                (Symbol::new(&e, "default_admin"), Symbol::new(&e, "other")).into_val(&e),
            ),
        ] {
            let before = e.to_ledger_snapshot().ledger_entries;
            assert!(e
                .try_invoke_contract::<Val, Error>(&id, &Symbol::new(&e, name), args)
                .is_err());
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
        }
    }
}
