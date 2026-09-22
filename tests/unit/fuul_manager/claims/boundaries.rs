use crate::test::{
    constructor_helpers::assert_diagnostic, roles_pause_helpers as auth, security_helpers::state, *,
};
use soroban_sdk::{Map, TryFromVal};

#[test]
fn cumulative_and_user_totals_accept_u256_max_then_reject_overflow_atomically() {
    totals_boundary(false);
}

#[test]
fn wasm_totals_accept_max_and_zero_reject_overflow_and_reset_expired_window() {
    totals_boundary(true);
}

fn totals_boundary(guest: bool) {
    let env = auth::test_env();
    let mut f = claim_fixture(&env);
    if guest {
        let id = env.register(
            constructor_helpers::MANAGER_WASM,
            (
                &f.admin,
                &f.pauser,
                &f.admin,
                1_u128,
                vec![&env, f.signer.clone()],
                &f.currency,
                &f.native_asset,
                None::<Address>,
                u(&env, 1_000_000_000_000_u128),
            ),
        );
        f.client = FuulManagerClient::new(&env, &id);
    }
    let project = register_mock_project(&env, &f.admin, 0);
    let max = U256::from_parts(&env, u64::MAX, u64::MAX, u64::MAX, u64::MAX);
    let before_max = U256::from_parts(&env, u64::MAX, u64::MAX, u64::MAX, u64::MAX - 1);
    env.as_contract(&f.client.address, || {
        storage::set_currency_limit(
            &env,
            &f.currency,
            &CurrencyTokenLimit {
                claim_limit_per_cooldown: max.clone(),
                cumulative_claim_per_cooldown: before_max.clone(),
                claim_cooldown_period_started: 1_000_000,
            },
        );
        storage::set_users_claims(&env, &f.recipient, &f.currency, &before_max);
    });
    for amount in [1, 0] {
        let checks = vec![&env, claim_check(&env, &f, &project.address, amount, 111)];
        security_helpers::authorize_claims(&env, &f.client.address, &f.caller, &checks, &[]);
        f.client.claim(&f.caller, &checks);
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), max);
        assert_eq!(f.client.currency_limits(&f.currency).cumulative_claim_per_cooldown, max);
        assert_eq!(project.last_recipient(), Some(f.recipient.clone()));
    }
    // This callee always raises 6102. A 6308 result proves it was never reached.
    let rejecting_project = env.register(ReplayRejectingProject, ());
    for timestamp in [1_000_000, 1_086_400] {
        env.ledger().with_mut(|l| l.timestamp = timestamp);
        let checks = vec![&env, claim_check(&env, &f, &rejecting_project, 1, 112)];
        security_helpers::authorize_claims(&env, &f.client.address, &f.caller, &checks, &[]);
        let before = state(&env, &f.client.address);
        let project_before = state(&env, &rejecting_project);
        assert_eq!(
            f.client.try_claim(&f.caller, &checks),
            Err(Ok(Error::from_contract_error(6308)))
        );
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
        assert_eq!(state(&env, &rejecting_project), project_before);
    }
    let negative = vec![&env, claim_check(&env, &f, &rejecting_project, -1, 113)];
    security_helpers::authorize_claims(&env, &f.client.address, &f.caller, &negative, &[]);
    let before = state(&env, &f.client.address);
    assert_eq!(f.client.try_claim(&f.caller, &negative), Err(Ok(Error::from_contract_error(6300))));
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &f.client.address), before);

    // The expired max window resets successfully when the independent user total fits.
    env.as_contract(&f.client.address, || {
        storage::set_users_claims(&env, &f.recipient, &f.currency, &u(&env, 7));
    });
    let checks = vec![&env, claim_check(&env, &f, &project.address, 1, 114)];
    security_helpers::authorize_claims(&env, &f.client.address, &f.caller, &checks, &[]);
    f.client.claim(&f.caller, &checks);
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 8));
    let window = f.client.currency_limits(&f.currency);
    assert_eq!(window.cumulative_claim_per_cooldown, u(&env, 1));
    assert_eq!(window.claim_cooldown_period_started, 1_086_400);
}

#[test]
fn invalid_claim_wire_enum_and_integer_widths_fail_before_business_writes() {
    for (field, bad) in
        [("currency_type", 0), ("reason", 0), ("amount", 1), ("deadline", 2), ("token_id", 1)]
    {
        let env = auth::test_env();
        let f = claim_fixture(&env);
        let project = register_mock_project(&env, &f.admin, 0);
        let check = claim_check(&env, &f, &project.address, 1, 113);
        let encoded: Val = check.into_val(&env);
        let mut wire = Map::<Symbol, Val>::try_from_val(&env, &encoded).unwrap();
        let value = match bad {
            0 => (Symbol::new(&env, "NotAVariant"),).into_val(&env),
            1 => (1_u128 << 127).into_val(&env),
            _ => (u128::from(u64::MAX) + 1).into_val(&env),
        };
        wire.set(Symbol::new(&env, field), value);
        let before = state(&env, &f.client.address);
        assert_eq!(
            env.try_invoke_contract::<(), Error>(
                &f.client.address,
                &Symbol::new(&env, "claim"),
                (&f.caller, vec![&env, wire]).into_val(&env)
            ),
            Err(Ok(auth::native_auth_error())),
            "{field}"
        );
        if bad == 0 {
            assert_diagnostic(&env, soroban_sdk::xdr::ScError::Value(ScErrorCode::InvalidInput));
        } else {
            assert_diagnostic(&env, soroban_sdk::xdr::ScError::WasmVm(ScErrorCode::InvalidAction));
            let diagnostics = env.host().get_diagnostic_events().unwrap();
            assert!(
                std::format!("{diagnostics:?}").contains("ConversionError"),
                "{field}: {diagnostics:?}"
            );
        }
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
        assert_eq!(project.last_recipient(), None);
    }
}
