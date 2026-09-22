use crate::test::{authority::*, creation_helpers::register, wire::*, *};
use soroban_sdk::{xdr::ScVal, Error, IntoVal, Symbol};

#[test]
fn source_fee_values_have_exact_getters_and_raw_events() {
    for compiled in [false, true] {
        let e = test_env();
        let f = fixture(&e);
        let id = register(&e, compiled, &f.project_wasm_hash, &f.admin);
        let c = FuulFactoryClient::new(&e, &id);
        let p =
            c.create_fuul_project(&f.project_admin, &String::from_str(&e, "ipfs://values"), &false);
        e.mock_all_auths();
        // Source native quantities are rescaled from 18 decimals to seven-decimal stroops.
        for value in [100_000_i128, 200_000] {
            c.set_default_native_claim_fee(&f.admin, &value);
            event(
                &e,
                &id,
                &[symbol("default_native_claim_fee_updated")],
                map(&[("new_default_native_claim_fee", signed(value))]),
            );
            assert_eq!(c.default_native_claim_fee(), value);
        }
        for value in [1_000_000_i128, 300_000] {
            c.set_native_user_claim_fee(&f.admin, &p, &value);
            event(
                &e,
                &id,
                &[symbol("native_claim_fee_updated")],
                map(&[("project_address", address(&p)), ("native_claim_fee", signed(value))]),
            );
            assert_eq!(c.project_fees(&p).native_user_claim_fee, value);
        }
        c.set_default_project_claim_fee(&f.admin, &200);
        event(
            &e,
            &id,
            &[symbol("DefaultProjectClaimFeeUpdated")],
            map(&[("new_project_claim_fee", ScVal::U32(200))]),
        );
        assert_eq!(c.default_project_claim_fee(), 200);
        for value in [1_000, 300] {
            c.set_project_claim_fee(&f.admin, &p, &value);
            event(
                &e,
                &id,
                &[symbol("project_claim_fee_updated")],
                map(&[("project_address", address(&p)), ("project_claim_fee", ScVal::U32(value))]),
            );
            assert_eq!(c.project_fees(&p).project_claim_fee, value);
        }
        for value in [500, 300] {
            c.set_default_remove_fee(&f.admin, &value);
            event(
                &e,
                &id,
                &[symbol("default_remove_fee_updated")],
                map(&[("default_remove_fee", ScVal::U32(value))]),
            );
            assert_eq!(c.default_remove_fee(), value);
        }
        for value in [1_000, 250] {
            c.set_remove_fee(&f.admin, &p, &value);
            event(
                &e,
                &id,
                &[symbol("remove_fee_updated")],
                map(&[("project_address", address(&p)), ("remove_fee", ScVal::U32(value))]),
            );
            assert_eq!(c.project_fees(&p).remove_fee, value);
        }
    }
}

#[test]
fn source_fee_setters_reject_authenticated_outsiders_without_changes() {
    for compiled in [false, true] {
        let e = test_env();
        let f = fixture(&e);
        let id = register(&e, compiled, &f.project_wasm_hash, &f.admin);
        let c = FuulFactoryClient::new(&e, &id);
        let p = c.create_fuul_project(
            &f.project_admin,
            &String::from_str(&e, "ipfs://outsider"),
            &false,
        );
        let outsider = Address::generate(&e);
        let cases: [(&str, Vec<soroban_sdk::Val>); 6] = [
            ("set_default_native_claim_fee", (&outsider, 100_000_i128).into_val(&e)),
            ("set_native_user_claim_fee", (&outsider, &p, 1_000_000_i128).into_val(&e)),
            ("set_default_project_claim_fee", (&outsider, 100_u32).into_val(&e)),
            ("set_project_claim_fee", (&outsider, &p, 1_000_u32).into_val(&e)),
            ("set_default_remove_fee", (&outsider, 100_u32).into_val(&e)),
            ("set_remove_fee", (&outsider, &p, 1_000_u32).into_val(&e)),
        ];
        for (name, args) in cases {
            authorize(&e, &id, &outsider, name, args.clone());
            let before = e.to_ledger_snapshot().ledger_entries;
            assert_eq!(
                e.try_invoke_contract::<(), Error>(&id, &Symbol::new(&e, name), args),
                Err(Ok(Error::from_contract_error(2000))),
                "{name}"
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
        }
    }
}

#[test]
fn source_snapshots_and_overrides_preserve_both_complete_records() {
    for compiled in [false, true] {
        let e = test_env();
        let f = fixture(&e);
        let id = register(&e, compiled, &f.project_wasm_hash, &f.admin);
        let c = FuulFactoryClient::new(&e, &id);
        e.mock_all_auths();
        c.set_default_native_claim_fee(&f.admin, &200_000);
        c.set_default_project_claim_fee(&f.admin, &200);
        let a = c.create_fuul_project(&f.project_admin, &String::from_str(&e, "ipfs://a"), &false);
        assert_eq!(
            c.project_fees(&a),
            ProjectFees { native_user_claim_fee: 200_000, project_claim_fee: 200, remove_fee: 0 }
        );
        c.set_default_native_claim_fee(&f.admin, &500_000);
        c.set_default_project_claim_fee(&f.admin, &500);
        let b = c.create_fuul_project(&f.project_admin, &String::from_str(&e, "ipfs://b"), &false);
        assert_eq!(
            c.project_fees(&b),
            ProjectFees { native_user_claim_fee: 500_000, project_claim_fee: 500, remove_fee: 0 }
        );
        c.set_native_user_claim_fee(&f.admin, &a, &1_000_000);
        c.set_project_claim_fee(&f.admin, &a, &1_000);
        c.set_remove_fee(&f.admin, &a, &500);
        c.set_remove_fee(&f.admin, &b, &1_000);
        assert_eq!(
            c.project_fees(&a),
            ProjectFees {
                native_user_claim_fee: 1_000_000,
                project_claim_fee: 1_000,
                remove_fee: 500
            }
        );
        assert_eq!(
            c.project_fees(&b),
            ProjectFees {
                native_user_claim_fee: 500_000,
                project_claim_fee: 500,
                remove_fee: 1_000
            }
        );
        c.set_default_native_claim_fee(&f.admin, &10_000);
        c.set_default_project_claim_fee(&f.admin, &300);
        c.set_default_remove_fee(&f.admin, &250);
        let complete = c.create_fuul_project(
            &f.project_admin,
            &String::from_str(&e, "ipfs://complete"),
            &false,
        );
        let expected =
            ProjectFees { native_user_claim_fee: 10_000, project_claim_fee: 300, remove_fee: 250 };
        assert_eq!(c.project_fees(&complete), expected);
        assert_eq!(c.get_fees_information(&complete).fees, expected);
        assert_eq!(c.default_native_claim_fee(), 10_000);
        assert_eq!(c.default_project_claim_fee(), 300);
        assert_eq!(c.default_remove_fee(), 250);
    }
}

#[test]
fn source_removal_boundaries_repeated_value_and_zero_reset() {
    for compiled in [false, true] {
        let e = test_env();
        let f = fixture(&e);
        let id = register(&e, compiled, &f.project_wasm_hash, &f.admin);
        let c = FuulFactoryClient::new(&e, &id);
        e.mock_all_auths();
        c.set_default_remove_fee(&f.admin, &500);
        let p =
            c.create_fuul_project(&f.project_admin, &String::from_str(&e, "ipfs://reset"), &false);
        assert_eq!(c.project_fees(&p).remove_fee, 500);
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            c.try_set_remove_fee(&f.admin, &p, &500),
            Err(Ok(Error::from_contract_error(6201)))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        for value in [100, 0] {
            c.set_remove_fee(&f.admin, &p, &value);
            assert_eq!(c.project_fees(&p).remove_fee, value);
        }
        for value in [5_000, 10_000] {
            c.set_default_remove_fee(&f.admin, &value);
            assert_eq!(c.default_remove_fee(), value);
        }
        for value in [10_001, 20_000] {
            let before = e.to_ledger_snapshot().ledger_entries;
            assert_eq!(
                c.try_set_default_remove_fee(&f.admin, &value),
                Err(Ok(Error::from_contract_error(6201)))
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
            assert_eq!(
                c.try_set_remove_fee(&f.admin, &p, &value),
                Err(Ok(Error::from_contract_error(6201)))
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
        }
        c.set_remove_fee(&f.admin, &p, &10_000);
        assert_eq!(c.project_fees(&p).remove_fee, 10_000);
    }
}
