use crate::test::{roles_pause_helpers::*, *};

#[test]
fn unpause_without_authorization_preserves_state() {
    let env = test_env();
    let f = fixture(&env);
    transition(&env, &f.client, &f.pauser, true);
    let accounts = [f.admin.clone(), f.pauser.clone(), f.unpauser.clone()];
    let before = state(&env, &f.client, &accounts);
    let ttl_before = ttl(&env, &f.client);
    env.set_auths(&[]);
    let diagnostics = diagnostic_count(&env);
    assert_eq!(f.client.try_unpause(&f.unpauser), Err(Ok(native_auth_error())));
    assert_auth_failure(&env, diagnostics);
    assert!(env.events().all().events().is_empty());
    assert_eq!(ttl(&env, &f.client), ttl_before);
    assert_eq!(state(&env, &f.client, &accounts), before);
}

fn rejected_grant(authenticated_outsider: bool) {
    let env = test_env();
    let f = fixture(&env);
    let account = Address::generate(&env);
    let caller = if authenticated_outsider { &f.pauser } else { &f.admin };
    let role = f.client.pauser_role();
    let accounts = [f.admin.clone(), f.pauser.clone(), f.unpauser.clone(), account.clone()];
    let before = state(&env, &f.client, &accounts);
    let ttl_before = ttl(&env, &f.client);
    if authenticated_outsider {
        authorize(
            &env,
            &f.client.address,
            caller,
            "grant_role",
            (&role, &account, caller).into_val(&env),
        );
    } else {
        env.set_auths(&[]);
    }
    let diagnostics = diagnostic_count(&env);
    let result = FuulAccessControlClient::new(&env, &f.client.address)
        .try_grant_role(&role, &account, caller);
    let expected =
        if authenticated_outsider { Error::from_contract_error(2000) } else { native_auth_error() };
    assert_eq!(result, Err(Ok(expected)));
    if !authenticated_outsider {
        assert_auth_failure(&env, diagnostics);
    }
    assert!(env.events().all().events().is_empty());
    assert_eq!(ttl(&env, &f.client), ttl_before);
    assert_eq!(state(&env, &f.client, &accounts), before);
}

#[test]
fn grant_role_requires_admin_authorization() {
    rejected_grant(false);
}
#[test]
fn authenticated_pauser_cannot_grant_roles() {
    rejected_grant(true);
}
