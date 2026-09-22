use crate::test::*;

mod actor_events;
mod authorization;
mod roles;
mod ttl;

#[test]
fn pauser_and_unpauser_roles_control_state_with_standard_events() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.pause(&fixture.pauser);
    assert_eq!(
        env.events().all(),
        std::vec![Paused { account: fixture.pauser.clone() }.to_xdr(&env, &fixture.client.address)]
    );
    assert!(fixture.client.paused());

    fixture.client.unpause(&fixture.unpauser);
    assert_eq!(
        env.events().all(),
        std::vec![
            Unpaused { account: fixture.unpauser.clone() }.to_xdr(&env, &fixture.client.address)
        ]
    );
    assert!(!fixture.client.paused());
}

#[test]
#[should_panic(expected = "Error(Contract, #2000)")]
fn non_pauser_role_cannot_pause() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.pause(&Address::generate(&env));
}

#[test]
#[should_panic(expected = "Error(Contract, #2000)")]
fn non_unpauser_role_cannot_unpause() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    fixture.client.pause(&fixture.pauser);

    fixture.client.unpause(&Address::generate(&env));
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn pause_requires_caller_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.pause(&fixture.pauser);
}

#[test]
#[should_panic(expected = "Error(Contract, #1000)")]
fn pause_rejects_an_already_paused_contract() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    fixture.client.pause(&fixture.pauser);

    fixture.client.pause(&fixture.pauser);
}

#[test]
#[should_panic(expected = "Error(Contract, #1001)")]
fn unpause_rejects_an_unpaused_contract() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.unpause(&fixture.unpauser);
}
