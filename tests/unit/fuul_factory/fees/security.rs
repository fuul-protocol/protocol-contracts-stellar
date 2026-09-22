use crate::test::{authority::*, *};
use fuul_core::INSTANCE_EXTEND_AMOUNT;
use soroban_sdk::{
    testutils::Ledger,
    xdr::{ContractEventBody, ScError, ScErrorCode, ScErrorType, ScVal},
    Error, IntoVal, Symbol, Val,
};

fn aged_fixture(e: &Env) -> Fixture<'_> {
    // Keep entries live while aging them below renewal thresholds.
    e.ledger().with_mut(|ledger| ledger.min_persistent_entry_ttl = INSTANCE_EXTEND_AMOUNT + 1);
    let f = fixture(e);
    e.mock_all_auths();
    f.client.set_native_user_claim_fee(&f.admin, &f.project_admin, &20_000);
    f.client.set_project_claim_fee(&f.admin, &f.project_admin, &100);
    f.client.set_remove_fee(&f.admin, &f.project_admin, &50);
    e.ledger().with_mut(|ledger| ledger.sequence_number += 70 * 17_280);
    f
}

#[test]
fn invalid_values_in_all_seven_setters_restore_business_state_ttls_and_events() {
    let e = test_env();
    let f = aged_fixture(&e);
    let calls: [(&str, Vec<Val>); 9] = [
        ("set_fee_collector", (&f.admin, &f.collector).into_val(&e)),
        ("set_default_native_claim_fee", (&f.admin, -1_i128).into_val(&e)),
        ("set_native_user_claim_fee", (&f.admin, &f.project_admin, -1_i128).into_val(&e)),
        ("set_default_native_claim_fee", (&f.admin, 20_000_i128).into_val(&e)),
        ("set_native_user_claim_fee", (&f.admin, &f.project_admin, 20_000_i128).into_val(&e)),
        ("set_default_project_claim_fee", (&f.admin, 10_001_u32).into_val(&e)),
        ("set_project_claim_fee", (&f.admin, &f.project_admin, 10_001_u32).into_val(&e)),
        ("set_default_remove_fee", (&f.admin, 10_001_u32).into_val(&e)),
        ("set_remove_fee", (&f.admin, &f.project_admin, 10_001_u32).into_val(&e)),
    ];
    for (name, args) in calls {
        authorize(&e, &f.client.address, &f.admin, name, args.clone());
        let before = state(&e, &f.client.address);
        assert_eq!(
            e.try_invoke_contract::<(), Error>(&f.client.address, &Symbol::new(&e, name), args),
            Err(Ok(Error::from_contract_error(6201))),
            "{name}"
        );
        assert!(e.events().all().events().is_empty());
        assert_eq!(state(&e, &f.client.address), before, "{name}");
    }
}

#[test]
fn revoked_admin_and_missing_or_wrong_auth_fail_before_fee_validation_without_ttl_changes() {
    for mode in 0..3 {
        let e = test_env();
        let f = aged_fixture(&e);
        let outsider = Address::generate(&e);
        if mode == 2 {
            e.mock_all_auths();
            let access = FuulAccessControlClient::new(&e, &f.client.address);
            access.revoke_role(&role(&e), &f.admin, &f.admin);
        }
        for (name, args) in setters(&e, &f.admin, &f.project_admin, &f.collector, 10_001) {
            e.mock_auths(&[]);
            if mode != 0 {
                authorize(
                    &e,
                    &f.client.address,
                    if mode == 1 { &outsider } else { &f.admin },
                    name,
                    args.clone(),
                );
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
                assert!(e.host().get_diagnostic_events().unwrap().0.iter().any(|event| {
                    let ContractEventBody::V0(body) = &event.event.body;
                    event.failed_call
                        && body
                            .topics
                            .contains(&ScVal::Error(ScError::Auth(ScErrorCode::InvalidAction)))
                }));
            }
            assert!(e.events().all().events().is_empty());
            assert_eq!(state(&e, &f.client.address), before);
        }
    }
}

#[test]
fn out_of_domain_fee_wire_values_fail_before_any_business_write() {
    let e = test_env();
    let f = fixture(&e);
    let calls: [(&str, Vec<Val>); 6] = [
        ("set_default_native_claim_fee", (&f.admin, 1_u128 << 127).into_val(&e)),
        ("set_native_user_claim_fee", (&f.admin, &f.project_admin, 1_u128 << 127).into_val(&e)),
        ("set_default_project_claim_fee", (&f.admin, u64::from(u32::MAX) + 1).into_val(&e)),
        (
            "set_project_claim_fee",
            (&f.admin, &f.project_admin, u64::from(u32::MAX) + 1).into_val(&e),
        ),
        ("set_default_remove_fee", (&f.admin, -1_i128).into_val(&e)),
        ("set_remove_fee", (&f.admin, &f.project_admin, -1_i128).into_val(&e)),
    ];
    e.mock_all_auths();
    for (name, args) in calls {
        let before = state(&e, &f.client.address);
        assert_eq!(
            e.try_invoke_contract::<(), Error>(&f.client.address, &Symbol::new(&e, name), args),
            Err(Ok(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction))),
            "{name}"
        );
        let diagnostics = e.host().get_diagnostic_events().unwrap();
        assert!(diagnostics.0.iter().any(|event| {
            let ContractEventBody::V0(body) = &event.event.body;
            event.failed_call
                && body.topics.contains(&ScVal::Error(ScError::WasmVm(ScErrorCode::InvalidAction)))
        }));
        // Host and SDK integer conversions report different diagnostic text.
        let detail = std::format!("{diagnostics:?}");
        assert!(
            detail.contains("UnexpectedType") || detail.contains("ConversionError"),
            "{name}: {detail}"
        );
        assert!(e.events().all().events().is_empty());
        assert_eq!(state(&e, &f.client.address), before);
    }
}
