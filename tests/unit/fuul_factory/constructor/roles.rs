use crate::test::*;
use soroban_sdk::Symbol;
use stellar_access::access_control as oz;

#[test]
fn constructor_seeds_default_admin_as_enumerable_membership() {
    let env = authority::test_env();
    let f = fixture(&env);
    env.as_contract(&f.client.address, || {
        let role = Symbol::new(&env, "default_admin");
        assert_eq!(oz::has_role(&env, &f.admin, &role), Some(0));
        assert_eq!(oz::get_role_member_count(&env, &role), 1);
        assert_eq!(oz::get_role_member(&env, &role, 0), f.admin);
    });
}

#[test]
fn constructor_emits_both_role_grants_in_source_order() {
    use stellar_access::access_control::RoleGranted;
    let e = authority::test_env();
    let f = fixture(&e);
    assert_eq!(
        e.events().all(),
        std::vec![
            RoleGranted {
                role: Symbol::new(&e, "default_admin"),
                account: f.admin.clone(),
                caller: f.admin.clone()
            }
            .to_xdr(&e, &f.client.address),
            RoleGranted {
                role: Symbol::new(&e, "manager"),
                account: f.manager.clone(),
                caller: f.admin
            }
            .to_xdr(&e, &f.client.address),
        ]
    );
}
