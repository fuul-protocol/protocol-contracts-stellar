use crate::test::{authority::*, *};
use soroban_sdk::{
    xdr::{ScErrorCode, ScErrorType},
    Error, IntoVal, Symbol,
};
use stellar_access::access_control::AccessControlStorageKey as Key;

#[test]
fn all_setters_authenticate_caller_before_validating_and_preserve_failed_state() {
    for mode in 0..3 {
        let e = test_env();
        let f = fixture(&e);
        let outsider = Address::generate(&e);
        let actor = if mode == 2 { &outsider } else { &f.admin };
        // Same collector and out-of-range BPS ensure auth precedes business validation.
        for (name, args) in setters(&e, actor, &f.project_admin, &f.collector, 10_001) {
            e.mock_auths(&[]);
            if mode != 0 {
                authorize(&e, &f.client.address, &outsider, name, args.clone());
            }
            let before = state(&e, &f.client.address);
            let expected = if mode == 2 {
                Error::from_contract_error(2000)
            } else {
                Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)
            };
            assert_eq!(
                e.try_invoke_contract::<(), Error>(&f.client.address, &Symbol::new(&e, name), args),
                Err(Ok(expected)),
                "{name}, mode {mode}"
            );
            if mode != 2 {
                use soroban_sdk::xdr::{ContractEventBody, ScError, ScVal};
                assert!(
                    e.host().get_diagnostic_events().unwrap().0.iter().any(|event| {
                        let ContractEventBody::V0(body) = &event.event.body;
                        event.failed_call
                            && body
                                .topics
                                .contains(&ScVal::Error(ScError::Auth(ScErrorCode::InvalidAction)))
                    }),
                    "missing auth diagnostic for {name}, mode {mode}"
                );
            }
            assert!(e.events().all().events().is_empty());
            assert_eq!(state(&e, &f.client.address), before);
        }
    }
}

#[test]
fn legacy_singleton_and_delegation_keys_do_not_grant_authority() {
    let e = test_env();
    let f = fixture(&e);
    let outsider = Address::generate(&e);
    e.as_contract(&f.client.address, || {
        e.storage().instance().set(&Key::Admin, &outsider);
        e.storage().persistent().set(&Key::RoleAdmin(role(&e)), &Symbol::new(&e, "manager"));
    });
    e.mock_all_auths();
    for actor in [&outsider, &f.manager] {
        assert_eq!(
            f.client.try_set_default_remove_fee(actor, &1),
            Err(Ok(Error::from_contract_error(2000)))
        );
    }
    f.client.set_default_remove_fee(&f.admin, &1);
}

#[test]
fn stellar_singleton_and_delegation_endpoints_are_not_exported() {
    let e = test_env();
    let f = fixture(&e);
    let args = [
        ("get_admin", Vec::new(&e)),
        ("get_existing_roles", Vec::new(&e)),
        ("renounce_admin", Vec::new(&e)),
        ("accept_admin_transfer", Vec::new(&e)),
        ("transfer_admin_role", (&f.admin, 100_u32).into_val(&e)),
        ("set_role_admin", (role(&e), role(&e)).into_val(&e)),
    ];
    e.mock_all_auths();
    for (name, args) in args {
        let before = state(&e, &f.client.address);
        assert!(e
            .try_invoke_contract::<soroban_sdk::Val, Error>(
                &f.client.address,
                &Symbol::new(&e, name),
                args
            )
            .is_err());
        assert_eq!(state(&e, &f.client.address), before);
        assert!(e.events().all().events().is_empty());
    }
}
