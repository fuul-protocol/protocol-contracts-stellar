use crate::test::{authority::*, creation_helpers::register, wire::*, *};
use soroban_sdk::{xdr::ScVal, IntoVal};

#[test]
fn factory_events_have_exact_raw_topics_data_and_integer_types() {
    for compiled in [false, true] {
        let e = test_env();
        let f = fixture(&e);
        let id = register(&e, compiled, &f.project_wasm_hash, &f.admin);
        let c = FuulFactoryClient::new(&e, &id);
        let uri = String::from_str(&e, "ipfs://events");
        let project = c.create_fuul_project(&f.project_admin, &uri, &false);
        event(
            &e,
            &id,
            &[symbol("project_created"), address(&project)],
            map(&[
                ("project_id", unsigned(1)),
                (
                    "project_info_uri",
                    ScVal::String(soroban_sdk::xdr::ScString("ipfs://events".try_into().unwrap())),
                ),
            ]),
        );
        e.mock_all_auths();
        c.set_fee_collector(&f.admin, &f.collector);
        event(&e, &id, &[symbol("fee_collector_updated"), address(&f.collector)], map(&[]));
        c.set_default_native_claim_fee(&f.admin, &7);
        event(
            &e,
            &id,
            &[symbol("default_native_claim_fee_updated")],
            map(&[("new_default_native_claim_fee", signed(7))]),
        );
        c.set_native_user_claim_fee(&f.admin, &project, &8);
        event(
            &e,
            &id,
            &[symbol("native_claim_fee_updated")],
            map(&[("project_address", address(&project)), ("native_claim_fee", signed(8))]),
        );
        c.set_default_project_claim_fee(&f.admin, &9);
        event(
            &e,
            &id,
            &[symbol("DefaultProjectClaimFeeUpdated")],
            map(&[("new_project_claim_fee", ScVal::U32(9))]),
        );
        c.set_project_claim_fee(&f.admin, &project, &10);
        event(
            &e,
            &id,
            &[symbol("project_claim_fee_updated")],
            map(&[("project_address", address(&project)), ("project_claim_fee", ScVal::U32(10))]),
        );
        c.set_default_remove_fee(&f.admin, &11);
        event(
            &e,
            &id,
            &[symbol("default_remove_fee_updated")],
            map(&[("default_remove_fee", ScVal::U32(11))]),
        );
        c.set_remove_fee(&f.admin, &project, &12);
        event(
            &e,
            &id,
            &[symbol("remove_fee_updated")],
            map(&[("project_address", address(&project)), ("remove_fee", ScVal::U32(12))]),
        );
    }
}

#[test]
fn role_events_identify_the_authenticated_actor() {
    for compiled in [false, true] {
        let e = test_env();
        let f = fixture(&e);
        let id = register(&e, compiled, &f.project_wasm_hash, &f.admin);
        let access = FuulAccessControlClient::new(&e, &id);
        let b = Address::generate(&e);
        authorize(&e, &id, &f.admin, "grant_role", (role(&e), &b, &f.admin).into_val(&e));
        access.grant_role(&role(&e), &b, &f.admin);
        event(
            &e,
            &id,
            &[symbol("role_granted"), symbol("default_admin"), address(&b)],
            map(&[("caller", address(&f.admin))]),
        );
        authorize(&e, &id, &b, "grant_role", (role(&e), &f.project_admin, &b).into_val(&e));
        access.grant_role(&role(&e), &f.project_admin, &b);
        assert_eq!(e.auths()[0].0, b);
        event(
            &e,
            &id,
            &[symbol("role_granted"), symbol("default_admin"), address(&f.project_admin)],
            map(&[("caller", address(&b))]),
        );
        authorize(&e, &id, &b, "revoke_role", (role(&e), &f.project_admin, &b).into_val(&e));
        access.revoke_role(&role(&e), &f.project_admin, &b);
        event(
            &e,
            &id,
            &[symbol("role_revoked"), symbol("default_admin"), address(&f.project_admin)],
            map(&[("caller", address(&b))]),
        );
    }
}
