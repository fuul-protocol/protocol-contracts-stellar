use crate::test::{authority::*, *};
use soroban_sdk::{Error, IntoVal, Symbol, Val};

#[test]
fn all_four_bps_setters_cover_zero_9999_10000_10001_and_same_value() {
    for name in [
        "set_default_project_claim_fee",
        "set_project_claim_fee",
        "set_default_remove_fee",
        "set_remove_fee",
    ] {
        let e = test_env();
        let f = fixture(&e);
        // An unregistered address is deliberately valid as an EVM mapping key.
        let project = Address::generate(&e);
        let args = |value: u32| -> Vec<Val> {
            if name.starts_with("set_default") {
                (&f.admin, value).into_val(&e)
            } else {
                (&f.admin, &project, value).into_val(&e)
            }
        };
        e.mock_all_auths();
        e.invoke_contract::<()>(&f.client.address, &Symbol::new(&e, name), args(1));
        for value in [0, 9_999, 10_000] {
            e.invoke_contract::<()>(&f.client.address, &Symbol::new(&e, name), args(value));
            assert_eq!(e.events().all().events().len(), 1);
            let actual = match name {
                "set_default_project_claim_fee" => f.client.default_project_claim_fee(),
                "set_default_remove_fee" => f.client.default_remove_fee(),
                "set_project_claim_fee" => f.client.project_fees(&project).project_claim_fee,
                _ => f.client.project_fees(&project).remove_fee,
            };
            assert_eq!(actual, value, "{name}");
            let before = state(&e, &f.client.address);
            assert_eq!(
                e.try_invoke_contract::<(), Error>(
                    &f.client.address,
                    &Symbol::new(&e, name),
                    args(value)
                ),
                Err(Ok(Error::from_contract_error(6201)))
            );
            assert!(e.events().all().events().is_empty());
            assert_eq!(state(&e, &f.client.address), before);
        }
        let before = state(&e, &f.client.address);
        assert_eq!(
            e.try_invoke_contract::<(), Error>(
                &f.client.address,
                &Symbol::new(&e, name),
                args(10_001)
            ),
            Err(Ok(Error::from_contract_error(6201)))
        );
        assert!(e.events().all().events().is_empty());
        assert_eq!(state(&e, &f.client.address), before);
    }
}

#[test]
fn both_fixed_fee_setters_accept_zero_and_i128_max_and_reject_negative_or_same() {
    for name in ["set_default_native_claim_fee", "set_native_user_claim_fee"] {
        let e = test_env();
        let f = fixture(&e);
        let args = |value: i128| -> Vec<Val> {
            if name == "set_default_native_claim_fee" {
                (&f.admin, value).into_val(&e)
            } else {
                (&f.admin, &f.project_admin, value).into_val(&e)
            }
        };
        e.mock_all_auths();
        e.invoke_contract::<()>(&f.client.address, &Symbol::new(&e, name), args(1));
        for value in [0, i128::MAX] {
            e.invoke_contract::<()>(&f.client.address, &Symbol::new(&e, name), args(value));
            assert_eq!(e.events().all().events().len(), 1);
            let actual = if name == "set_default_native_claim_fee" {
                f.client.default_native_claim_fee()
            } else {
                f.client.project_fees(&f.project_admin).native_user_claim_fee
            };
            assert_eq!(actual, value);
            for rejected in [value, -1] {
                let before = state(&e, &f.client.address);
                assert_eq!(
                    e.try_invoke_contract::<(), Error>(
                        &f.client.address,
                        &Symbol::new(&e, name),
                        args(rejected)
                    ),
                    Err(Ok(Error::from_contract_error(6201)))
                );
                assert!(e.events().all().events().is_empty());
                assert_eq!(state(&e, &f.client.address), before);
            }
        }
    }
}

#[test]
fn unknown_project_overrides_preserve_other_keys_and_all_defaults() {
    let e = test_env();
    let f = fixture(&e);
    let unknown = Address::generate(&e);
    let other = Address::generate(&e);
    e.mock_all_auths();
    f.client.set_native_user_claim_fee(&f.admin, &unknown, &45_000);
    f.client.set_project_claim_fee(&f.admin, &unknown, &9_999);
    f.client.set_remove_fee(&f.admin, &unknown, &10_000);
    assert_eq!(
        f.client.get_fees_information(&unknown),
        FeesInformation {
            fee_collector: f.collector,
            fees: ProjectFees {
                native_user_claim_fee: 45_000,
                project_claim_fee: 9_999,
                remove_fee: 10_000,
            },
        }
    );
    assert_eq!(
        f.client.project_fees(&other),
        ProjectFees { native_user_claim_fee: 0, project_claim_fee: 0, remove_fee: 0 }
    );
    assert_eq!(
        (
            f.client.default_native_claim_fee(),
            f.client.default_project_claim_fee(),
            f.client.default_remove_fee()
        ),
        (20_000, 100, 0)
    );
    assert_eq!(f.client.contract_tracker(), 0);
}
