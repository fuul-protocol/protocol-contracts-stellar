use crate::test::{
    constructor_helpers::*, roles_pause_helpers as auth, security_helpers::state, *,
};
use fuul_core::{INSTANCE_EXTEND_AMOUNT as EXTEND, INSTANCE_TTL_THRESHOLD as THRESHOLD};
use soroban_sdk::testutils::{storage::Persistent, Deployer};
use stellar_access::access_control::{
    AccessControlStorageKey as OzKey, ROLE_EXTEND_AMOUNT, ROLE_TTL_THRESHOLD,
};

fn aged_env() -> Env {
    let env = auth::test_env();
    // Keep every role/index alive when aging the instance; no archival-as-absence shortcut.
    env.ledger().with_mut(|l| l.min_persistent_entry_ttl = EXTEND + 1);
    env
}

fn age_to(env: &Env, id: &Address, ttl: u32) {
    let current = env.deployer().get_contract_instance_ttl(id);
    env.ledger().with_mut(|l| l.sequence_number += current - ttl);
    assert_eq!(env.deployer().get_contract_instance_ttl(id), ttl);
}

fn persistent_ttl(env: &Env, id: &Address, key: &Val) -> u32 {
    env.as_contract(id, || env.storage().persistent().get_ttl(key))
}

#[test]
fn configuration_getters_preserve_business_state_and_obey_instance_ttl_threshold() {
    for ttl in [THRESHOLD - 1, THRESHOLD, THRESHOLD + 1] {
        for name in [
            "claim_cooldown",
            "required_signers",
            "kyc_validator",
            "native_asset",
            "min_claim_cooldown",
            "pauser_role",
            "unpauser_role",
            "claim_signer_role",
            "default_admin_role",
        ] {
            let env = aged_env();
            let f = fixture(&env);
            age_to(&env, &f.client.address, ttl);
            let before = state(&env, &f.client.address);
            let _: Val =
                env.invoke_contract(&f.client.address, &Symbol::new(&env, name), vec![&env]);
            assert!(env.events().all().events().is_empty());
            let after = state(&env, &f.client.address);
            assert_eq!(
                before.iter().map(|(k, e, _)| (k, e)).collect::<std::vec::Vec<_>>(),
                after.iter().map(|(k, e, _)| (k, e)).collect::<std::vec::Vec<_>>(),
                "{name}"
            );
            let renews = matches!(
                name,
                "claim_cooldown" | "required_signers" | "kyc_validator" | "native_asset"
            );
            let expected = if renews && ttl <= THRESHOLD { EXTEND } else { ttl };
            assert_eq!(
                env.deployer().get_contract_instance_ttl(&f.client.address),
                expected,
                "{name}"
            );
            assert_eq!(env.deployer().get_contract_code_ttl(&f.client.address), expected, "{name}");
            assert_eq!(
                persistent_ttl(&env, &f.client.address, &limit_key(&env, &f.accepted_currency)),
                ttl
            );
        }
    }
}

#[test]
fn persistent_getters_renew_only_present_nonzero_or_true_entries_at_threshold() {
    for ttl in [THRESHOLD - 1, THRESHOLD, THRESHOLD + 1] {
        let env = aged_env();
        let f = fixture(&env);
        let user = Address::generate(&env);
        let zero = Address::generate(&env);
        let absent = Address::generate(&env);
        let zero_currency = Address::generate(&env);
        let exempt_key: Val = (Symbol::new(&env, "FeeExemption"), &user).into_val(&env);
        let false_key: Val = (Symbol::new(&env, "FeeExemption"), &zero).into_val(&env);
        let total_key: Val =
            (Symbol::new(&env, "UserClaims"), &user, &f.accepted_currency).into_val(&env);
        let zero_key: Val =
            (Symbol::new(&env, "UserClaims"), &zero, &f.accepted_currency).into_val(&env);
        env.as_contract(&f.client.address, || {
            storage::set_no_claim_fee_address(&env, &user, true);
            env.storage().persistent().set(&false_key, &false);
            storage::set_users_claims(&env, &user, &f.accepted_currency, &u(&env, 7));
            storage::set_users_claims(&env, &zero, &f.accepted_currency, &u(&env, 0));
            storage::set_currency_limit(&env, &zero_currency, &CurrencyTokenLimit::zero(&env));
        });
        age_to(&env, &f.client.address, ttl);
        let before = state(&env, &f.client.address);
        for key in [&exempt_key, &false_key, &total_key, &zero_key] {
            assert_eq!(persistent_ttl(&env, &f.client.address, key), ttl);
        }
        assert!(f.client.no_claim_fee_addresses(&user));
        assert!(!f.client.no_claim_fee_addresses(&zero));
        assert!(!f.client.no_claim_fee_addresses(&absent));
        assert_eq!(f.client.users_claims(&user, &f.accepted_currency), u(&env, 7));
        assert_eq!(f.client.users_claims(&zero, &f.accepted_currency), u(&env, 0));
        assert_eq!(f.client.users_claims(&absent, &f.accepted_currency), u(&env, 0));
        assert_eq!(f.client.currency_limits(&absent), CurrencyTokenLimit::zero(&env));
        assert_eq!(f.client.currency_limits(&zero_currency), CurrencyTokenLimit::zero(&env));
        assert_eq!(
            f.client.currency_limits(&f.accepted_currency).claim_limit_per_cooldown,
            u(&env, 1_000_000_000_000_i128)
        );
        let after = state(&env, &f.client.address);
        assert_eq!(
            before.iter().map(|(k, e, _)| (k, e)).collect::<std::vec::Vec<_>>(),
            after.iter().map(|(k, e, _)| (k, e)).collect::<std::vec::Vec<_>>()
        );
        let expected = if ttl <= THRESHOLD { EXTEND } else { ttl };
        assert_eq!(
            persistent_ttl(&env, &f.client.address, &limit_key(&env, &zero_currency)),
            expected
        );
        for key in [&exempt_key, &total_key, &limit_key(&env, &f.accepted_currency)] {
            assert_eq!(persistent_ttl(&env, &f.client.address, key), expected);
        }
        for key in [&false_key, &zero_key] {
            assert_eq!(persistent_ttl(&env, &f.client.address, key), ttl);
        }
        assert_eq!(env.deployer().get_contract_instance_ttl(&f.client.address), ttl);
    }
}

#[test]
fn zero_total_write_renews_but_zero_total_read_does_not() {
    let env = aged_env();
    let f = fixture(&env);
    let key: Val =
        (Symbol::new(&env, "UserClaims"), &f.signer, &f.accepted_currency).into_val(&env);
    env.as_contract(&f.client.address, || {
        storage::set_users_claims(&env, &f.signer, &f.accepted_currency, &u(&env, 0))
    });
    age_to(&env, &f.client.address, THRESHOLD - 1);
    assert_eq!(persistent_ttl(&env, &f.client.address, &key), THRESHOLD - 1);
    assert_eq!(f.client.users_claims(&f.signer, &f.accepted_currency), u(&env, 0));
    assert_eq!(persistent_ttl(&env, &f.client.address, &key), THRESHOLD - 1);
    env.as_contract(&f.client.address, || {
        storage::set_users_claims(&env, &f.signer, &f.accepted_currency, &u(&env, 0))
    });
    assert_eq!(persistent_ttl(&env, &f.client.address, &key), EXTEND);
}

#[test]
fn keep_alive_renews_instance_and_current_code_without_renewing_role_entries() {
    for ttl in [THRESHOLD - 1, THRESHOLD, THRESHOLD + 1] {
        let env = aged_env();
        let f = fixture(&env);
        env.mock_all_auths();
        let role = Symbol::new(&env, "claim_signer");
        env.as_contract(&f.client.address, || {
            env.storage()
                .persistent()
                .set(&OzKey::RoleAdmin(role.clone()), &Symbol::new(&env, "pauser"))
        });
        age_to(&env, &f.client.address, ttl);
        let keys: [Val; 5] = [
            OzKey::ExistingRoles.into_val(&env),
            OzKey::HasRole(f.signer.clone(), role.clone()).into_val(&env),
            OzKey::RoleAccountsCount(role.clone()).into_val(&env),
            role_member_key(&env, &role, 0),
            OzKey::RoleAdmin(role).into_val(&env),
        ];
        for key in &keys {
            assert_eq!(persistent_ttl(&env, &f.client.address, key), ttl);
        }
        let before = state(&env, &f.client.address);
        env.mock_auths(&[]);
        f.client.keep_alive();
        assert!(env.events().all().events().is_empty());
        let after = state(&env, &f.client.address);
        assert_eq!(
            before.iter().map(|(k, e, _)| (k, e)).collect::<std::vec::Vec<_>>(),
            after.iter().map(|(k, e, _)| (k, e)).collect::<std::vec::Vec<_>>()
        );
        let expected = if ttl <= THRESHOLD { EXTEND } else { ttl };
        assert_eq!(env.deployer().get_contract_instance_ttl(&f.client.address), expected);
        assert_eq!(env.deployer().get_contract_code_ttl(&f.client.address), expected);
        for key in &keys {
            assert_eq!(persistent_ttl(&env, &f.client.address, key), ttl);
        }
    }
}

#[test]
fn oz_getters_renew_only_the_selected_role_key_at_the_oz_threshold() {
    for ttl in [ROLE_TTL_THRESHOLD - 1, ROLE_TTL_THRESHOLD, ROLE_TTL_THRESHOLD + 1] {
        for operation in 0..5 {
            let env = aged_env();
            let f = fixture(&env);
            let access = FuulAccessControlClient::new(&env, &f.client.address);
            let role = Symbol::new(&env, "claim_signer");
            env.mock_all_auths();
            env.as_contract(&f.client.address, || {
                env.storage()
                    .persistent()
                    .set(&OzKey::RoleAdmin(role.clone()), &Symbol::new(&env, "pauser"))
            });
            age_to(&env, &f.client.address, ttl);
            let keys: [Val; 5] = [
                OzKey::ExistingRoles.into_val(&env),
                OzKey::HasRole(f.signer.clone(), role.clone()).into_val(&env),
                OzKey::RoleAccountsCount(role.clone()).into_val(&env),
                role_member_key(&env, &role, 0),
                OzKey::RoleAdmin(role.clone()).into_val(&env),
            ];
            for key in &keys {
                assert_eq!(persistent_ttl(&env, &f.client.address, key), ttl);
            }
            let before = state(&env, &f.client.address);
            match operation {
                0 => assert_eq!(access.get_role_members(&role), vec![&env, f.signer.clone()]),
                1 => assert!(access.has_role(&role, &f.signer)),
                2 => assert_eq!(access.get_role_member_count(&role), 1),
                3 => assert_eq!(access.get_role_member(&role, &0), f.signer),
                _ => assert_eq!(access.get_role_admin(&role), Symbol::new(&env, "default_admin")),
            }
            let after = state(&env, &f.client.address);
            assert_eq!(
                before.iter().map(|(k, e, _)| (k, e)).collect::<std::vec::Vec<_>>(),
                after.iter().map(|(k, e, _)| (k, e)).collect::<std::vec::Vec<_>>()
            );
            for (index, key) in keys.iter().enumerate() {
                assert_eq!(
                    persistent_ttl(&env, &f.client.address, key),
                    if ((operation == 0 && matches!(index, 2 | 3))
                        || (operation > 0 && operation < 4 && index == operation))
                        && ttl <= ROLE_TTL_THRESHOLD
                    {
                        ROLE_EXTEND_AMOUNT
                    } else {
                        ttl
                    }
                );
            }
            assert_eq!(env.deployer().get_contract_instance_ttl(&f.client.address), ttl);
        }
    }
}

#[test]
fn seven_configuration_mutators_renew_only_their_expected_storage_at_threshold() {
    for ttl in [THRESHOLD - 1, THRESHOLD, THRESHOLD + 1] {
        for operation in 0..7 {
            let env = aged_env();
            let f = fixture(&env);
            let account = Address::generate(&env);
            let exempt: Val = (Symbol::new(&env, "FeeExemption"), &account).into_val(&env);
            env.mock_all_auths();
            if operation == 5 {
                f.client.add_no_claim_fee_address(&f.admin, &account);
            }
            age_to(&env, &f.client.address, ttl);
            let native_key = limit_key(&env, &f.native_asset);
            assert_eq!(persistent_ttl(&env, &f.client.address, &native_key), ttl);
            let (name, mut args): (&str, Vec<Val>) = match operation {
                0 => ("set_claim_cooldown", (172_800_u128,).into_val(&env)),
                1 => ("set_required_signers", (2_u128,).into_val(&env)),
                2 => ("add_currency_limit", (&account, u(&env, 1)).into_val(&env)),
                3 => {
                    ("set_currency_token_limit", (&f.accepted_currency, u(&env, 1)).into_val(&env))
                }
                4 => ("add_no_claim_fee_address", (&account,).into_val(&env)),
                5 => ("remove_no_claim_fee_address", (&account,).into_val(&env)),
                _ => ("set_kyc_validator", (None::<Address>,).into_val(&env)),
            };
            args.push_front(f.admin.clone().into_val(&env));
            auth::authorize(&env, &f.client.address, &f.admin, name, args.clone());
            env.invoke_contract::<()>(&f.client.address, &Symbol::new(&env, name), args);
            let expected = if ttl <= THRESHOLD { EXTEND } else { ttl };
            assert_eq!(
                env.deployer().get_contract_instance_ttl(&f.client.address),
                if matches!(operation, 0 | 1 | 6) { expected } else { ttl }
            );
            assert_eq!(persistent_ttl(&env, &f.client.address, &native_key), ttl);
            match operation {
                2 => assert_eq!(
                    persistent_ttl(&env, &f.client.address, &limit_key(&env, &account)),
                    EXTEND
                ),
                3 => assert_eq!(
                    persistent_ttl(&env, &f.client.address, &limit_key(&env, &f.accepted_currency)),
                    expected
                ),
                4 => assert_eq!(persistent_ttl(&env, &f.client.address, &exempt), EXTEND),
                5 => env.as_contract(&f.client.address, || {
                    assert!(!env.storage().persistent().has(&exempt))
                }),
                _ => (),
            }
        }
    }
}
