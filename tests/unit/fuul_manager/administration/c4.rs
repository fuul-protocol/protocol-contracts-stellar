use crate::test::{roles_pause_helpers as auth, security_helpers::state, *};
use stellar_access::access_control::AccessControlStorageKey as OzKey;

#[path = "../../../utils/role_capacity.rs"]
mod role_capacity;

#[test]
fn absent_role_removal_is_a_noop_after_authentication() {
    for renounce in [false, true] {
        let env = auth::test_env();
        let f = fixture(&env);
        let access = FuulAccessControlClient::new(&env, &f.client.address);
        assert!(access.has_role(&access.default_admin_role(), &f.admin));
        let account = Address::generate(&env);
        let role = Symbol::new(&env, "unknown");
        let (name, args, actor): (&str, Vec<Val>, &Address) = if renounce {
            ("renounce_role", (&role, &account).into_val(&env), &account)
        } else {
            ("revoke_role", (&role, &account, &f.admin).into_val(&env), &f.admin)
        };
        env.mock_auths(&[]);
        let before = state(&env, &f.client.address);
        assert_eq!(
            env.try_invoke_contract::<(), Error>(
                &f.client.address,
                &Symbol::new(&env, name),
                args.clone()
            ),
            Err(Ok(auth::native_auth_error()))
        );
        auth::assert_auth_failure(&env, 0);
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
        auth::authorize(&env, &f.client.address, actor, name, args);
        let result = if renounce {
            access.try_renounce_role(&role, &account)
        } else {
            access.try_revoke_role(&role, &account, &f.admin)
        };
        assert_eq!(result, Ok(Ok(())));
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
    }
}

#[test]
fn manager_role_type_capacity_native() {
    role_type_capacity(false);
}

#[test]
fn manager_role_type_capacity_wasm() {
    role_type_capacity(true);
}

fn role_type_capacity(guest: bool) {
    let env = auth::test_env();
    role_capacity::prepare(&env);
    let input = constructor_helpers::Bootstrap::new(&env);
    let id = if guest {
        env.register(constructor_helpers::MANAGER_WASM, input.args(&env))
    } else {
        input.register(&env)
    };
    role_capacity::assert_capacity(&env, &id, &input.admin, 4);
}

#[test]
fn fresh_manager_does_not_store_singleton_admin_authority() {
    let env = auth::test_env();
    let f = fixture(&env);
    env.as_contract(&f.client.address, || {
        assert!(!env.storage().instance().has(&OzKey::Admin));
        assert!(!env.storage().temporary().has(&OzKey::PendingAdmin));
    });
}

#[test]
fn default_admin_public_getter_identifies_the_constructor_role() {
    let env = auth::test_env();
    let f = fixture(&env);
    assert_eq!(
        env.try_invoke_contract::<Symbol, Error>(
            &f.client.address,
            &Symbol::new(&env, "default_admin_role"),
            Vec::new(&env)
        ),
        Ok(Ok(Symbol::new(&env, "default_admin")))
    );
}
