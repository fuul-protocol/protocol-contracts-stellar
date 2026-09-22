use crate::test::authority::*;
use crate::test::*;
use soroban_sdk::Symbol;
use soroban_sdk::{vec, Error, IntoVal};
use stellar_access::access_control::{RoleGranted, RoleRevoked};

#[path = "../../../utils/role_capacity.rs"]
mod role_capacity;

#[test]
fn renouncing_an_absent_role_is_an_authenticated_no_op() {
    let env = test_env();
    let f = fixture(&env);
    let access = FuulAccessControlClient::new(&env, &f.client.address);
    env.mock_all_auths();
    assert_eq!(access.try_renounce_role(&Symbol::new(&env, "absent"), &f.admin), Ok(Ok(())));
    assert!(env.events().all().events().is_empty());
}

#[test]
fn simultaneous_admins_self_revoke_and_last_admin_exit_match_evm() {
    let e = test_env();
    let f = fixture(&e);
    let a = FuulAccessControlClient::new(&e, &f.client.address);
    let b = Address::generate(&e);
    e.mock_all_auths();
    a.grant_role(&role(&e), &b, &f.admin);
    assert_eq!(
        e.events().all(),
        std::vec![RoleGranted { role: role(&e), account: b.clone(), caller: f.admin.clone() }
            .to_xdr(&e, &f.client.address)]
    );
    assert_eq!(a.get_role_members(&role(&e)), vec![&e, f.admin.clone(), b.clone()]);
    for (actor, value) in [(&f.admin, 1), (&b, 2)] {
        let collector = Address::generate(&e);
        for (name, args) in setters(&e, actor, &f.project_admin, &collector, value) {
            authorize(&e, &f.client.address, actor, name, args.clone());
            e.invoke_contract::<()>(&f.client.address, &Symbol::new(&e, name), args);
            assert_eq!(e.events().all().events().len(), 1);
        }
        assert_eq!(f.client.default_native_claim_fee(), i128::from(value));
        assert_eq!(f.client.default_project_claim_fee(), value);
        assert_eq!(f.client.default_remove_fee(), value);
        assert_eq!(
            f.client.project_fees(&f.project_admin),
            ProjectFees {
                native_user_claim_fee: i128::from(value),
                project_claim_fee: value,
                remove_fee: value
            }
        );
        assert_eq!(f.client.fee_collector(), collector);
    }
    e.mock_all_auths();
    a.revoke_role(&role(&e), &f.admin, &f.admin);
    assert_eq!(
        e.events().all(),
        std::vec![RoleRevoked {
            role: role(&e),
            account: f.admin.clone(),
            caller: f.admin.clone()
        }
        .to_xdr(&e, &f.client.address)]
    );
    assert_eq!(a.get_role_members(&role(&e)), vec![&e, b.clone()]);
    assert_eq!(
        f.client.try_set_default_remove_fee(&f.admin, &3),
        Err(Ok(Error::from_contract_error(2000)))
    );
    a.renounce_role(&role(&e), &b);
    assert_eq!(a.get_role_member_count(&role(&e)), 0);
    assert_eq!(
        f.client.try_set_default_remove_fee(&b, &3),
        Err(Ok(Error::from_contract_error(2000)))
    );
    assert_eq!(
        a.try_grant_role(&role(&e), &f.admin, &b),
        Err(Ok(Error::from_contract_error(2000)))
    );
}

#[test]
fn role_enumeration_noops_and_unknown_admin_role_match_evm() {
    let e = test_env();
    let f = fixture(&e);
    let a = FuulAccessControlClient::new(&e, &f.client.address);
    let r = Symbol::new(&e, "custom");
    let b = Address::generate(&e);
    let c = Address::generate(&e);
    e.mock_all_auths();
    for member in [&f.admin, &b, &c] {
        a.grant_role(&r, member, &f.admin);
    }
    a.revoke_role(&r, &b, &f.admin);
    assert_eq!(a.get_role_members(&r), vec![&e, f.admin.clone(), c.clone()]);
    assert!(!a.has_role(&r, &b));
    assert!(a.has_role(&r, &c));
    assert_eq!(a.get_role_member(&r, &1), c);
    assert_eq!(a.try_get_role_member(&r, &2), Err(Ok(Error::from_contract_error(2002))));
    assert_eq!(a.get_role_admin(&Symbol::new(&e, "")), role(&e));
    a.grant_role(&r, &c, &f.admin);
    assert!(e.events().all().events().is_empty());
    a.revoke_role(&r, &b, &f.admin);
    assert!(e.events().all().events().is_empty());
    assert_eq!(a.get_role_member_count(&r), 2);
}

#[test]
fn factory_role_type_capacity_native() {
    role_type_capacity(false);
}

#[test]
fn factory_role_type_capacity_wasm() {
    role_type_capacity(true);
}

fn role_type_capacity(guest: bool) {
    let e = test_env();
    role_capacity::prepare(&e);
    let f = fixture(&e);
    let id = if guest {
        e.register(
            &include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../target/wasm32v1-none/release/fuul_factory.wasm"
            ))[..],
            (&f.admin, &f.manager, &f.collector, &f.project_wasm_hash),
        )
    } else {
        f.client.address.clone()
    };
    role_capacity::assert_capacity(&e, &id, &f.admin, 2);
}

#[test]
fn absent_role_noops_still_require_exact_caller_authentication() {
    let e = test_env();
    let f = fixture(&e);
    let r = Symbol::new(&e, "missing");
    let calls: [(&str, Vec<soroban_sdk::Val>); 2] = [
        ("revoke_role", (&r, &f.manager, &f.admin).into_val(&e)),
        ("renounce_role", (&r, &f.admin).into_val(&e)),
    ];
    for (name, args) in calls {
        e.mock_auths(&[]);
        let before = state(&e, &f.client.address);
        assert!(e
            .try_invoke_contract::<(), Error>(
                &f.client.address,
                &Symbol::new(&e, name),
                args.clone()
            )
            .is_err());
        assert_eq!(state(&e, &f.client.address), before);
        assert!(e.events().all().events().is_empty());
        authorize(&e, &f.client.address, &f.admin, name, args.clone());
        e.invoke_contract::<()>(&f.client.address, &Symbol::new(&e, name), args);
        assert!(e.events().all().events().is_empty());
    }
}
