use super::*;
use soroban_sdk::testutils::storage::{Instance, Persistent};

#[test]
fn initialization_assigns_distinct_roles_and_project_configuration() {
    let e = Env::default();
    let f = Fixture::new(&e);
    for (contract, account, role) in [
        (&f.manager.address, &f.admin, "default_admin"),
        (&f.manager.address, &f.pauser, "pauser"),
        (&f.manager.address, &f.unpauser, "unpauser"),
        (&f.manager.address, &f.signer, "claim_signer"),
        (&f.factory.address, &f.factory_admin, "default_admin"),
        (&f.factory.address, &f.manager.address, "manager"),
        (&f.project.address, &f.project_admin, "default_admin"),
    ] {
        assert!(
            FuulAccessControlClient::new(&e, contract).has_role(&Symbol::new(&e, role), account)
        );
    }
    assert!(!FuulAccessControlClient::new(&e, &f.project.address)
        .has_role(&Symbol::new(&e, "default_admin"), &f.factory_admin));
    assert_eq!(f.manager.required_signers(), 1);
    assert_eq!(f.manager.claim_cooldown(), 86_400);
    assert_eq!(f.manager.native_asset(), f.native);
    assert_eq!(f.project.factory(), f.factory.address);
    assert_eq!(f.project.project_info_uri(), String::from_str(&e, "ipfs://project"));
    assert!(!f.project.kyc_required());
    assert_eq!(f.factory.contract_tracker(), 1);
    assert_eq!(f.factory.fee_collector(), f.collector);
}

#[test]
fn invalid_constructors_leave_no_roles_limits_or_events() {
    for case in 0..7 {
        let e = Env::default();
        let id = Address::generate(&e);
        let admin = Address::generate(&e);
        let signer = Address::generate(&e);
        let currency = Address::generate(&e);
        let mut native = Address::generate(&e);
        let mut signers = vec![&e, signer.clone()];
        let mut quorum = 1_u128;
        let mut limit = u(&e, 100);
        let code = match case {
            0 => {
                quorum = 0;
                6300
            }
            1 => {
                quorum = 1_u128 << 96;
                6300
            }
            2 => {
                signers = vec![&e];
                6300
            }
            3 => {
                quorum = 2;
                6300
            }
            4 => {
                signers.push_back(signer);
                6301
            }
            5 => {
                native = currency.clone();
                6302
            }
            _ => {
                limit = u(&e, 0);
                6300
            }
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            e.register_at(
                &id,
                crate::FuulManager,
                (
                    &admin,
                    &admin,
                    &admin,
                    quorum,
                    signers,
                    &currency,
                    native,
                    None::<Address>,
                    limit,
                ),
            )
        }));
        assert!(result.is_err());
        // The diagnostic proves this was the intended contract error, not an unrelated panic.
        let diagnostic = std::format!("{:?}", e.host().get_diagnostic_events().unwrap());
        assert!(diagnostic.contains(&std::format!("Contract({code})")), "{diagnostic}");
        assert!(e.events().all().events().is_empty());
        e.as_contract(&id, || {
            assert!(e.storage().instance().all().is_empty());
            assert!(e.storage().persistent().all().is_empty());
        });
    }
}

#[test]
fn factory_creation_is_unique_and_invalid_uri_does_not_advance_tracker() {
    let e = Env::default();
    let f = Fixture::new(&e);
    assert_eq!(
        f.factory.try_create_fuul_project(&f.project_admin, &String::from_str(&e, ""), &false),
        Err(Ok(error(6200)))
    );
    assert_eq!(f.factory.contract_tracker(), 1);
    let other = f.factory.create_fuul_project(
        &f.project_admin,
        &String::from_str(&e, "ipfs://second"),
        &true,
    );
    assert_ne!(other, f.project.address);
    assert_eq!(f.factory.contract_tracker(), 2);
    assert!(FuulProjectClient::new(&e, &other).kyc_required());
}

#[test]
fn factory_fee_setters_enforce_bounds_roles_and_project_snapshots() {
    for case in 0..6 {
        let e = Env::default();
        let f = Fixture::new(&e);
        let set = |caller: &Address, value: i128| match case {
            0 => f.factory.try_set_default_project_claim_fee(caller, &(value as u32)),
            1 => f.factory.try_set_project_claim_fee(caller, &f.project.address, &(value as u32)),
            2 => f.factory.try_set_default_remove_fee(caller, &(value as u32)),
            3 => f.factory.try_set_remove_fee(caller, &f.project.address, &(value as u32)),
            4 => f.factory.try_set_default_native_claim_fee(caller, &value),
            _ => f.factory.try_set_native_user_claim_fee(caller, &f.project.address, &value),
        };
        assert_eq!(set(&f.caller, 250), Err(Ok(error(2000))));
        assert_eq!(set(&f.factory_admin, if case < 4 { 10_001 } else { -1 }), Err(Ok(error(6201))));
        assert!(set(&f.factory_admin, 250).is_ok());
        assert_eq!(set(&f.factory_admin, 250), Err(Ok(error(6201))));
        assert!(set(&f.factory_admin, 0).is_ok());
        assert!(set(&f.factory_admin, if case < 4 { 10_000 } else { i128::MAX }).is_ok());
    }
    let e = Env::default();
    let f = Fixture::new(&e);
    f.factory.set_default_project_claim_fee(&f.factory_admin, &250);
    assert_eq!(f.factory.project_fees(&f.project.address).project_claim_fee, 100);
    let next =
        f.factory.create_fuul_project(&f.project_admin, &String::from_str(&e, "next"), &false);
    assert_eq!(f.factory.project_fees(&next).project_claim_fee, 250);
}

#[test]
fn management_requires_caller_authorization_and_current_roles() {
    let e = Env::default();
    let f = Fixture::new(&e);
    e.mock_auths(&[]);
    assert!(f.manager.try_set_required_signers(&f.admin, &2).is_err());
    assert!(f.factory.try_set_fee_collector(&f.factory_admin, &f.caller).is_err());
    assert!(f.project.try_set_project_uri(&f.project_admin, &String::from_str(&e, "new")).is_err());
    e.mock_all_auths();
    assert_eq!(f.manager.try_set_required_signers(&f.caller, &2), Err(Ok(error(2000))));
    assert_eq!(f.factory.try_set_fee_collector(&f.caller, &f.caller), Err(Ok(error(2000))));
    assert_eq!(
        f.project.try_set_project_uri(&f.caller, &String::from_str(&e, "new")),
        Err(Ok(error(2000)))
    );
    let access = FuulAccessControlClient::new(&e, &f.manager.address);
    let role = Symbol::new(&e, "default_admin");
    access.grant_role(&role, &f.caller, &f.admin);
    access.revoke_role(&role, &f.admin, &f.caller);
    assert_eq!(f.manager.try_set_required_signers(&f.admin, &2), Err(Ok(error(2000))));
    f.manager.set_required_signers(&f.caller, &2);
    assert_eq!(f.manager.required_signers(), 2);
}

#[test]
fn pause_and_unpause_have_separate_authorities() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let c = f.check(20, 100);
    assert_eq!(f.manager.try_pause(&f.admin), Err(Ok(error(2000))));
    f.manager.pause(&f.pauser);
    assert!(f.manager.paused());
    assert!(f.manager.try_claim(&f.caller, &vec![&e, c.clone()]).is_err());
    f.assert_unsettled(&c);
    assert_eq!(f.manager.try_unpause(&f.pauser), Err(Ok(error(2000))));
    f.manager.unpause(&f.unpauser);
    assert!(!f.manager.paused());
    f.claim(&c);
}

#[test]
fn manager_configuration_validates_numeric_boundaries() {
    let e = Env::default();
    let f = Fixture::new(&e);
    for value in [0, 1, 1_u128 << 96, u128::MAX] {
        assert_eq!(f.manager.try_set_required_signers(&f.admin, &value), Err(Ok(error(6300))));
    }
    for period in [0, 86_399, 86_400] {
        assert_eq!(f.manager.try_set_claim_cooldown(&f.admin, &period), Err(Ok(error(6300))));
    }
    f.manager.set_claim_cooldown(&f.admin, &u128::MAX);
    assert_eq!(f.manager.claim_cooldown(), u128::MAX);
    assert_eq!(
        f.manager.try_add_currency_limit(&f.admin, &f.currency, &u(&e, 1)),
        Err(Ok(error(6302)))
    );
    let other = Address::generate(&e);
    assert_eq!(f.manager.try_add_currency_limit(&f.admin, &other, &u(&e, 0)), Err(Ok(error(6300))));
    assert_eq!(
        f.manager.try_set_currency_token_limit(&f.admin, &other, &u(&e, 1)),
        Err(Ok(error(6300)))
    );
    f.manager.add_currency_limit(&f.admin, &other, &u(&e, u128::MAX));
    assert_eq!(f.manager.currency_limits(&other).claim_limit_per_cooldown, u(&e, u128::MAX));
}
