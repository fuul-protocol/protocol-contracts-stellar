use crate::test::{authority::*, wire::*, *};
use soroban_sdk::{
    vec,
    xdr::{ScBytes, ScVal},
    Error, IntoVal, Symbol, TryFromVal, Val,
};

#[test]
fn configuration_and_fee_getters_preserve_explicit_wire_types() {
    let e = test_env();
    let f = fixture(&e);
    for (name, getter, expected) in [
        ("ContractTracker", "contract_tracker", unsigned(0)),
        ("DefaultNativeClaimFee", "default_native_claim_fee", signed(20_000)),
        ("DefaultProjectClaimFee", "default_project_claim_fee", ScVal::U32(100)),
        ("DefaultRemoveFee", "default_remove_fee", ScVal::U32(0)),
        ("FeeCollector", "fee_collector", address(&f.collector)),
        (
            "ProjectWasmHash",
            "project_wasm_hash",
            ScVal::Bytes(ScBytes(f.project_wasm_hash.to_array().to_vec().try_into().unwrap())),
        ),
    ] {
        let stored = e.as_contract(&f.client.address, || {
            e.storage().instance().get::<_, Val>(&key(&e, name)).unwrap()
        });
        assert_eq!(ScVal::try_from_val(&e, &stored).unwrap(), expected);
        assert_eq!(result(&e, &f.client.address, getter, Vec::new(&e)), expected);
    }
    assert_eq!(result(&e, &f.client.address, "manager_role", Vec::new(&e)), symbol("manager"));
    assert_eq!(
        result(&e, &f.client.address, "has_manager_role", (&f.manager,).into_val(&e)),
        ScVal::Bool(true)
    );
    assert_eq!(
        result(&e, &f.client.address, "has_role", (role(&e), &f.admin).into_val(&e)),
        ScVal::Bool(true)
    );
    assert_eq!(
        result(&e, &f.client.address, "get_role_admin", (Symbol::new(&e, "unknown"),).into_val(&e)),
        symbol("default_admin")
    );
    assert_eq!(
        result(&e, &f.client.address, "get_fees_information", (&f.project_admin,).into_val(&e)),
        map(&[("fee_collector", address(&f.collector)), ("fees", fees([0, 0, 0]))])
    );
    e.mock_all_auths();
    f.client.set_native_user_claim_fee(&f.admin, &f.project_admin, &7);
    let raw = e.as_contract(&f.client.address, || {
        e.storage().persistent().get::<_, Val>(&fee_key(&e, &f.project_admin)).unwrap()
    });
    assert_eq!(ScVal::try_from_val(&e, &raw).unwrap(), fees([7, 0, 0]));
    assert_eq!(
        result(&e, &f.client.address, "project_fees", vec![&e, f.project_admin.into_val(&e)]),
        fees([7, 0, 0])
    );
}

#[test]
fn absent_optional_values_return_typed_defaults_without_creating_records() {
    let e = test_env();
    let f = fixture(&e);
    for (name, getter, expected) in [
        ("ContractTracker", "contract_tracker", unsigned(0)),
        ("DefaultNativeClaimFee", "default_native_claim_fee", signed(0)),
        ("DefaultProjectClaimFee", "default_project_claim_fee", ScVal::U32(0)),
        ("DefaultRemoveFee", "default_remove_fee", ScVal::U32(0)),
    ] {
        e.as_contract(&f.client.address, || e.storage().instance().remove(&key(&e, name)));
        assert_eq!(result(&e, &f.client.address, getter, Vec::new(&e)), expected);
        e.as_contract(&f.client.address, || assert!(!e.storage().instance().has(&key(&e, name))));
    }
    let before = state(&e, &f.client.address);
    assert_eq!(
        result(&e, &f.client.address, "project_fees", (&f.project_admin,).into_val(&e)),
        fees([0, 0, 0])
    );
    assert_eq!(
        result(&e, &f.client.address, "get_fees_information", (&f.project_admin,).into_val(&e)),
        map(&[("fee_collector", address(&f.collector)), ("fees", fees([0, 0, 0]))])
    );
    assert_eq!(state(&e, &f.client.address), before);
    assert!(e.events().all().events().is_empty());
}

#[test]
fn missing_required_configuration_traps_without_replacing_it_with_defaults() {
    for (name, getter, message) in [
        ("FeeCollector", "fee_collector", "fee collector must be initialized"),
        ("ProjectWasmHash", "project_wasm_hash", "project Wasm hash must be initialized"),
    ] {
        let e = test_env();
        let f = fixture(&e);
        e.as_contract(&f.client.address, || e.storage().instance().remove(&key(&e, name)));
        let before = state(&e, &f.client.address);
        assert_eq!(
            e.try_invoke_contract::<Val, Error>(
                &f.client.address,
                &Symbol::new(&e, getter),
                Vec::new(&e)
            )
            .unwrap_err(),
            Ok(Error::from_type_and_code(
                soroban_sdk::xdr::ScErrorType::Context,
                soroban_sdk::xdr::ScErrorCode::InvalidAction
            ))
        );
        assert!(std::format!("{:?}", e.host().get_diagnostic_events().unwrap()).contains(message));
        assert_eq!(state(&e, &f.client.address), before);
        assert!(e.events().all().events().is_empty());
    }
}
