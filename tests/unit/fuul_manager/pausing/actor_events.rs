use crate::test::{constructor_helpers::*, roles_pause_helpers::*, *};
use soroban_sdk::{
    testutils::storage::{Instance, Persistent},
    xdr::ContractEventBody,
    Map,
};

fn assert_actor_event(env: &Env, name: &str, account: &Address) {
    let events = env.events().all();
    assert_eq!(events.events().len(), 1, "one authoritative transition event");
    let ContractEventBody::V0(body) = &events.events()[0].body;
    assert_eq!(body.topics.to_vec(), std::vec![Symbol::new(env, name).into()]);
    assert_eq!(
        body.data,
        Map::from_array(env, [(symbol_short!("account"), account.clone())]).into()
    );
}

fn actor_events(guest: bool) {
    let env = test_env();
    let input = Bootstrap::new(&env);
    let id =
        if guest { env.register(MANAGER_WASM, input.args(&env)) } else { input.register(&env) };
    let client = FuulManagerClient::new(&env, &id);
    for (pause, actor, topic) in
        [(true, &input.pauser, "paused"), (false, &input.unpauser, "unpaused")]
    {
        transition(&env, &client, actor, pause);
        assert_actor_event(&env, topic, actor);
        assert_eq!(
            env.auths(),
            std::vec![(
                actor.clone(),
                AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        id.clone(),
                        Symbol::new(&env, if pause { "pause" } else { "unpause" }),
                        (actor,).into_val(&env)
                    )),
                    sub_invocations: std::vec![],
                }
            )]
        );
        assert_eq!(client.paused(), pause);
        env.as_contract(&id, || {
            assert_eq!(
                env.storage().instance().get::<_, bool>(&instance_key(&env, "Paused")),
                Some(pause)
            );
            assert_eq!(stellar_contract_utils::pausable::paused(&env), pause);
        });
    }
}

#[test]
fn native_transitions_emit_exact_authenticated_actor() {
    actor_events(false);
}

#[test]
fn wasm_transitions_emit_exact_authenticated_actor() {
    actor_events(true);
}

#[test]
fn rejected_transitions_preserve_storage_and_emit_no_event() {
    for guest in [false, true] {
        for pause in [false, true] {
            for failure in ["missing_auth", "wrong_role", "wrong_actor", "invalid_transition"] {
                let env = test_env();
                let input = Bootstrap::new(&env);
                let id = if guest {
                    env.register(MANAGER_WASM, input.args(&env))
                } else {
                    input.register(&env)
                };
                let client = FuulManagerClient::new(&env, &id);
                if !pause || failure == "invalid_transition" {
                    transition(&env, &client, &input.pauser, true);
                }
                if !pause && failure == "invalid_transition" {
                    transition(&env, &client, &input.unpauser, false);
                }
                let caller = if failure == "wrong_role" {
                    &input.admin
                } else if pause {
                    &input.pauser
                } else {
                    &input.unpauser
                };
                let name = if pause { "pause" } else { "unpause" };
                let before = env.as_contract(&id, || {
                    (
                        env.storage().instance().all(),
                        env.storage().persistent().all(),
                        env.storage().instance().get_ttl(),
                    )
                });
                if failure == "missing_auth" {
                    env.set_auths(&[]);
                } else {
                    let authorizer = if failure == "wrong_actor" { &input.admin } else { caller };
                    authorize(&env, &id, authorizer, name, (caller,).into_val(&env));
                }
                let result =
                    if pause { client.try_pause(caller) } else { client.try_unpause(caller) };
                let expected = match failure {
                    "wrong_role" => Error::from_contract_error(2000),
                    "invalid_transition" => {
                        Error::from_contract_error(if pause { 1000 } else { 1001 })
                    }
                    _ => native_auth_error(),
                };
                assert_eq!(result, Err(Ok(expected)), "{guest}/{pause}/{failure}");
                assert!(env.events().all().events().is_empty());
                let after = env.as_contract(&id, || {
                    (
                        env.storage().instance().all(),
                        env.storage().persistent().all(),
                        env.storage().instance().get_ttl(),
                    )
                });
                assert_eq!(after, before);
            }
        }
    }
}

#[test]
fn existing_oz_pause_key_remains_readable_and_writable() {
    let env = test_env();
    let f = fixture(&env);
    env.as_contract(&f.client.address, || stellar_contract_utils::pausable::pause(&env));
    assert!(f.client.paused());
    transition(&env, &f.client, &f.unpauser, false);
    assert_actor_event(&env, "unpaused", &f.unpauser);
    env.as_contract(&f.client.address, || {
        stellar_contract_utils::pausable::when_not_paused(&env);
        assert_eq!(
            env.storage().instance().get::<_, bool>(&instance_key(&env, "Paused")),
            Some(false)
        );
    });
}
