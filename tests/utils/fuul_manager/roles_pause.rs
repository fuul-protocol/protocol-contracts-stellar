use crate::test::*;
use soroban_sdk::testutils::{storage::Instance, EnvTestConfig};

pub(super) fn test_env() -> Env {
    Env::new_with_config(EnvTestConfig { capture_snapshot_at_drop: false })
}

pub(super) fn authorize(
    env: &Env,
    contract: &Address,
    caller: &Address,
    name: &str,
    args: Vec<Val>,
) {
    env.mock_auths(&[MockAuth {
        address: caller,
        invoke: &MockAuthInvoke { contract, fn_name: name, args, sub_invokes: &[] },
    }]);
}

pub(super) fn transition(env: &Env, client: &FuulManagerClient<'_>, caller: &Address, pause: bool) {
    authorize(
        env,
        &client.address,
        caller,
        if pause { "pause" } else { "unpause" },
        (caller,).into_val(env),
    );
    if pause {
        client.pause(caller);
    } else {
        client.unpause(caller);
    }
}

pub(super) fn grant(env: &Env, f: &Fixture<'_>, account: &Address, role: &Symbol) {
    authorize(
        env,
        &f.client.address,
        &f.admin,
        "grant_role",
        (role, account, &f.admin).into_val(env),
    );
    FuulAccessControlClient::new(env, &f.client.address).grant_role(role, account, &f.admin);
}

pub(super) fn ttl(env: &Env, client: &FuulManagerClient<'_>) -> u32 {
    env.as_contract(&client.address, || env.storage().instance().get_ttl())
}

pub(super) fn state(
    env: &Env,
    client: &FuulManagerClient<'_>,
    accounts: &[Address],
) -> (bool, std::vec::Vec<bool>) {
    let access = FuulAccessControlClient::new(env, &client.address);
    let roles = [Symbol::new(env, "pauser"), Symbol::new(env, "unpauser")];
    (
        client.paused(),
        accounts
            .iter()
            .flat_map(|account| roles.iter().map(|role| access.has_role(role, account)))
            .collect(),
    )
}

// Native try_* maps a host authorization failure to Context/InvalidAction.
// Inspect this invocation's typed diagnostic topics to establish the underlying cause.
pub(super) fn diagnostic_count(env: &Env) -> usize {
    env.host().get_diagnostic_events().unwrap().0.len()
}

pub(super) fn assert_auth_failure(env: &Env, since: usize) {
    use soroban_sdk::xdr::{ContractEventBody, ScError, ScVal};
    let diagnostics = env.host().get_diagnostic_events().unwrap();
    assert!(
        diagnostics.0.iter().skip(since).any(|event| {
            let ContractEventBody::V0(body) = &event.event.body;
            event.failed_call
                && body.topics.contains(&ScVal::Error(ScError::Auth(ScErrorCode::InvalidAction)))
        }),
        "missing Auth/InvalidAction diagnostic: {diagnostics:?}"
    );
}

pub(super) fn native_auth_error() -> Error {
    Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)
}
