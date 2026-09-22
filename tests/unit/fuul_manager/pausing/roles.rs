use crate::test::{roles_pause_helpers::*, *};

fn rejected_role(pause: bool, actor: u8, revoked: bool) {
    let env = test_env();
    let f = fixture(&env);
    let caller = match actor {
        0 => &f.admin,
        1 => &f.pauser,
        _ => &f.unpauser,
    };
    let role = if pause { f.client.pauser_role() } else { f.client.unpauser_role() };
    if !pause {
        transition(&env, &f.client, &f.pauser, true);
    }
    if revoked {
        authorize(
            &env,
            &f.client.address,
            &f.admin,
            "revoke_role",
            (&role, caller, &f.admin).into_val(&env),
        );
        FuulAccessControlClient::new(&env, &f.client.address).revoke_role(&role, caller, &f.admin);
    }
    let accounts = [f.admin.clone(), f.pauser.clone(), f.unpauser.clone()];
    let before = state(&env, &f.client, &accounts);
    let ttl_before = ttl(&env, &f.client);
    authorize(
        &env,
        &f.client.address,
        caller,
        if pause { "pause" } else { "unpause" },
        (caller,).into_val(&env),
    );
    let result = if pause { f.client.try_pause(caller) } else { f.client.try_unpause(caller) };
    assert_eq!(result, Err(Ok(Error::from_contract_error(2000))));
    assert!(env.events().all().events().is_empty());
    assert_eq!(ttl(&env, &f.client), ttl_before);
    assert_eq!(state(&env, &f.client, &accounts), before);
}

#[test]
fn revoked_pauser_cannot_pause() {
    rejected_role(true, 1, true);
}
#[test]
fn revoked_unpauser_cannot_unpause() {
    rejected_role(false, 2, true);
}
#[test]
fn unpauser_cannot_pause() {
    rejected_role(true, 2, false);
}
#[test]
fn pauser_cannot_unpause() {
    rejected_role(false, 1, false);
}
#[test]
fn admin_without_pauser_role_cannot_pause() {
    rejected_role(true, 0, false);
}
#[test]
fn admin_without_unpauser_role_cannot_unpause() {
    rejected_role(false, 0, false);
}

fn newly_granted_role(pause: bool) {
    let env = test_env();
    let f = fixture(&env);
    let account = Address::generate(&env);
    let role = if pause { f.client.pauser_role() } else { f.client.unpauser_role() };
    if !pause {
        transition(&env, &f.client, &f.pauser, true);
    }
    let access = FuulAccessControlClient::new(&env, &f.client.address);
    assert!(!access.has_role(&role, &account));
    grant(&env, &f, &account, &role);
    assert!(access.has_role(&role, &account));
    transition(&env, &f.client, &account, pause);
    let expected = if pause {
        Paused { account: account.clone() }.to_xdr(&env, &f.client.address)
    } else {
        Unpaused { account: account.clone() }.to_xdr(&env, &f.client.address)
    };
    assert_eq!(env.events().all(), std::vec![expected]);
    assert_eq!(
        env.auths(),
        std::vec![(
            account.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    f.client.address.clone(),
                    Symbol::new(&env, if pause { "pause" } else { "unpause" }),
                    (&account,).into_val(&env)
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
    assert_eq!(f.client.paused(), pause);
    assert!(access.has_role(&role, &account));
}

#[test]
fn newly_granted_pauser_can_pause() {
    newly_granted_role(true);
}
#[test]
fn newly_granted_unpauser_can_unpause() {
    newly_granted_role(false);
}
