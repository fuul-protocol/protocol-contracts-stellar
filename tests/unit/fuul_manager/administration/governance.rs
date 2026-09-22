use crate::test::{roles_pause_helpers as auth, security_helpers::state, *};
use stellar_access::access_control::{AccessControlStorageKey as OzKey, RoleGranted, RoleRevoked};

#[test]
fn unknown_and_empty_roles_have_default_admin_and_enumerable_members() {
    let env = auth::test_env();
    let f = fixture(&env);
    let access = FuulAccessControlClient::new(&env, &f.client.address);
    let admin_role = access.default_admin_role();
    for role in [Symbol::new(&env, "unknown"), Symbol::new(&env, ""), admin_role.clone()] {
        assert_eq!(access.get_role_admin(&role), admin_role);
    }
    let role = Symbol::new(&env, "unknown");
    assert_eq!(access.get_role_member_count(&role), 0);
    assert_eq!(access.get_role_members(&role), Vec::<Address>::new(&env));
    assert!(!access.has_role(&role, &f.admin));
    assert_eq!(access.try_get_role_member(&role, &0), Err(Ok(Error::from_contract_error(2002))));
    assert_eq!(access.get_role_members(&admin_role), vec![&env, f.admin.clone()]);
}

#[test]
fn role_self_revocation_is_allowed_but_outsiders_cannot_revoke() {
    let env = auth::test_env();
    let f = fixture(&env);
    let access = FuulAccessControlClient::new(&env, &f.client.address);
    let role = f.client.claim_signer_role();
    env.mock_all_auths();
    access.grant_role(&role, &f.admin, &f.admin);
    auth::authorize(
        &env,
        &f.client.address,
        &f.admin,
        "revoke_role",
        (&role, &f.admin, &f.admin).into_val(&env),
    );
    access.revoke_role(&role, &f.admin, &f.admin);
    assert_eq!(
        env.events().all(),
        std::vec![RoleRevoked {
            role: role.clone(),
            account: f.admin.clone(),
            caller: f.admin.clone()
        }
        .to_xdr(&env, &f.client.address)]
    );
    assert!(!access.has_role(&role, &f.admin));
    assert!(access.has_role(&access.default_admin_role(), &f.admin));
    let outsider = Address::generate(&env);
    for target in [&f.signer, &outsider] {
        auth::authorize(
            &env,
            &f.client.address,
            &outsider,
            "revoke_role",
            (&role, target, &outsider).into_val(&env),
        );
        let before = state(&env, &f.client.address);
        assert_eq!(
            access.try_revoke_role(&role, target, &outsider),
            Err(Ok(Error::from_contract_error(2000)))
        );
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
    }
}

#[test]
fn governance_mutations_require_the_bound_admin_or_member_authorization() {
    for operation in 0..3 {
        let env = auth::test_env();
        let f = fixture(&env);
        let outsider = Address::generate(&env);
        let role = f.client.claim_signer_role();
        let (name, args, actor): (&str, Vec<Val>, Address) = match operation {
            0 => ("grant_role", (&role, &outsider, &f.admin).into_val(&env), f.admin.clone()),
            1 => ("revoke_role", (&role, &f.signer, &f.admin).into_val(&env), f.admin.clone()),
            _ => ("renounce_role", (&role, &f.signer).into_val(&env), f.signer.clone()),
        };
        for wrong_actor in [false, true] {
            env.mock_auths(&[]);
            if wrong_actor {
                auth::authorize(&env, &f.client.address, &outsider, name, args.clone());
            }
            let before = state(&env, &f.client.address);
            assert_eq!(
                env.try_invoke_contract::<(), Error>(
                    &f.client.address,
                    &Symbol::new(&env, name),
                    args.clone()
                ),
                Err(Ok(auth::native_auth_error()))
            );
            auth::assert_auth_failure(&env, 0);
            assert!(env.events().all().events().is_empty());
            assert_eq!(state(&env, &f.client.address), before);
        }
        auth::authorize(&env, &f.client.address, &actor, name, args.clone());
        assert_eq!(
            env.try_invoke_contract::<(), Error>(&f.client.address, &Symbol::new(&env, name), args),
            Ok(Ok(()))
        );
        assert_eq!(env.events().all().events().len(), 1);
    }
}

#[test]
fn granting_an_admin_preserves_existing_admins_without_acceptance() {
    let env = auth::test_env();
    let f = fixture(&env);
    let access = FuulAccessControlClient::new(&env, &f.client.address);
    let role = access.default_admin_role();
    let next = Address::generate(&env);
    auth::authorize(
        &env,
        &f.client.address,
        &f.admin,
        "grant_role",
        (&role, &next, &f.admin).into_val(&env),
    );
    access.grant_role(&role, &next, &f.admin);
    assert_eq!(
        env.events().all(),
        std::vec![RoleGranted {
            role: role.clone(),
            account: next.clone(),
            caller: f.admin.clone()
        }
        .to_xdr(&env, &f.client.address)]
    );
    assert_eq!(access.get_role_members(&role), vec![&env, f.admin.clone(), next.clone()]);
    auth::authorize(
        &env,
        &f.client.address,
        &next,
        "set_required_signers",
        (&next, 2_u128).into_val(&env),
    );
    f.client.set_required_signers(&next, &2);
    auth::authorize(
        &env,
        &f.client.address,
        &f.admin,
        "set_required_signers",
        (&f.admin, 3_u128).into_val(&env),
    );
    f.client.set_required_signers(&f.admin, &3);
    auth::authorize(
        &env,
        &f.client.address,
        &next,
        "revoke_role",
        (&role, &f.admin, &next).into_val(&env),
    );
    access.revoke_role(&role, &f.admin, &next);
    assert_eq!(
        env.events().all(),
        std::vec![RoleRevoked {
            role: role.clone(),
            account: f.admin.clone(),
            caller: next.clone()
        }
        .to_xdr(&env, &f.client.address)]
    );
    assert_eq!(access.get_role_members(&role), vec![&env, next]);
    auth::authorize(
        &env,
        &f.client.address,
        &f.admin,
        "set_required_signers",
        (&f.admin, 4_u128).into_val(&env),
    );
    let before = state(&env, &f.client.address);
    assert_eq!(
        f.client.try_set_required_signers(&f.admin, &4),
        Err(Ok(Error::from_contract_error(2000)))
    );
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &f.client.address), before);
}

#[test]
fn removed_singleton_and_delegation_endpoints_are_unavailable_in_native_and_wasm() {
    for compiled in [false, true] {
        let env = auth::test_env();
        let input = constructor_helpers::Bootstrap::new(&env);
        let id = if compiled {
            env.register(constructor_helpers::MANAGER_WASM, input.args(&env))
        } else {
            input.register(&env)
        };
        env.mock_all_auths();
        for (name, args) in [
            ("get_admin", Vec::new(&env)),
            ("get_existing_roles", Vec::new(&env)),
            (
                "set_role_admin",
                (Symbol::new(&env, "claim_signer"), Symbol::new(&env, "delegate")).into_val(&env),
            ),
            ("transfer_admin_role", (&input.pauser, 100_u32).into_val(&env)),
            ("accept_admin_transfer", Vec::new(&env)),
            ("renounce_admin", Vec::new(&env)),
        ] {
            let before = state(&env, &id);
            assert_eq!(
                env.try_invoke_contract::<(), Error>(&id, &Symbol::new(&env, name), args),
                Err(Ok(auth::native_auth_error())),
                "{name}, wasm={compiled}"
            );
            constructor_helpers::assert_diagnostic(
                &env,
                if compiled {
                    soroban_sdk::xdr::ScError::WasmVm(ScErrorCode::MissingValue)
                } else {
                    soroban_sdk::xdr::ScError::Context(ScErrorCode::MissingValue)
                },
            );
            assert!(env.events().all().events().is_empty());
            assert_eq!(state(&env, &id), before);
        }
    }
}

#[test]
fn custom_roles_cannot_manage_signers_or_configuration() {
    let env = auth::test_env();
    let f = fixture(&env);
    let access = FuulAccessControlClient::new(&env, &f.client.address);
    let delegate = Address::generate(&env);
    let role = Symbol::new(&env, "signer_admin");
    env.mock_all_auths();
    access.grant_role(&role, &delegate, &f.admin);
    let signer_role = f.client.claim_signer_role();
    let calls: [(&str, Vec<Val>); 3] = [
        ("grant_role", (&signer_role, &delegate, &delegate).into_val(&env)),
        ("revoke_role", (&signer_role, &f.signer, &delegate).into_val(&env)),
        ("set_required_signers", (&delegate, 2_u128).into_val(&env)),
    ];
    for (name, args) in calls {
        auth::authorize(&env, &f.client.address, &delegate, name, args.clone());
        let before = state(&env, &f.client.address);
        assert_eq!(
            env.try_invoke_contract::<(), Error>(&f.client.address, &Symbol::new(&env, name), args),
            Err(Ok(Error::from_contract_error(2000)))
        );
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
    }
}

#[test]
fn signer_swap_remove_updates_indices_and_last_signer_can_renounce() {
    let env = auth::test_env();
    let f = fixture(&env);
    env.mock_all_auths();
    let access = FuulAccessControlClient::new(&env, &f.client.address);
    let role = f.client.claim_signer_role();
    let middle = Address::generate(&env);
    let last = Address::generate(&env);
    access.grant_role(&role, &middle, &f.admin);
    access.grant_role(&role, &last, &f.admin);
    assert!(access.has_role(&role, &last));
    let before = state(&env, &f.client.address);
    access.grant_role(&role, &last, &f.admin);
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &f.client.address), before);
    assert_eq!(access.get_role_member_count(&role), 3);
    access.revoke_role(&role, &middle, &f.admin);
    assert_eq!(
        env.events().all(),
        std::vec![RoleRevoked {
            role: role.clone(),
            account: middle.clone(),
            caller: f.admin.clone()
        }
        .to_xdr(&env, &f.client.address)]
    );
    assert!(access.has_role(&role, &last));
    assert_eq!(access.get_role_member(&role, &1), last);
    assert_eq!(access.get_role_members(&role), vec![&env, f.signer.clone(), last.clone()]);
    assert!(!access.has_role(&role, &middle));
    env.as_contract(&f.client.address, || {
        assert_eq!(
            env.storage().persistent().get::<_, u32>(&OzKey::HasRole(last.clone(), role.clone())),
            Some(1)
        )
    });
    access.revoke_role(&role, &last, &f.admin);
    auth::authorize(
        &env,
        &f.client.address,
        &f.signer,
        "renounce_role",
        (&role, &f.signer).into_val(&env),
    );
    access.renounce_role(&role, &f.signer);
    assert_eq!(
        env.events().all(),
        std::vec![RoleRevoked {
            role: role.clone(),
            account: f.signer.clone(),
            caller: f.signer.clone()
        }
        .to_xdr(&env, &f.client.address)]
    );
    assert_eq!(access.get_role_member_count(&role), 0);
    assert_eq!(access.get_role_members(&role), Vec::<Address>::new(&env));
    assert_eq!(f.client.required_signers(), 1);
}

#[test]
fn last_admin_can_revoke_or_renounce_and_all_setters_lose_authority() {
    for renounce in [false, true] {
        let env = auth::test_env();
        let f = fixture(&env);
        let access = FuulAccessControlClient::new(&env, &f.client.address);
        let role = access.default_admin_role();
        env.mock_all_auths();
        if renounce {
            access.renounce_role(&role, &f.admin);
        } else {
            access.revoke_role(&role, &f.admin, &f.admin);
        }
        assert_eq!(
            env.events().all(),
            std::vec![RoleRevoked {
                role: role.clone(),
                account: f.admin.clone(),
                caller: f.admin.clone()
            }
            .to_xdr(&env, &f.client.address)]
        );
        assert_eq!(access.get_role_member_count(&role), 0);
        for (name, args) in setter_calls(&env, &f, &f.admin) {
            auth::authorize(&env, &f.client.address, &f.admin, name, args.clone());
            let before = state(&env, &f.client.address);
            assert_eq!(
                env.try_invoke_contract::<(), Error>(
                    &f.client.address,
                    &Symbol::new(&env, name),
                    args
                ),
                Err(Ok(Error::from_contract_error(2000))),
                "{name}"
            );
            assert!(env.events().all().events().is_empty());
            assert_eq!(state(&env, &f.client.address), before);
        }
        assert!(access.has_role(&f.client.claim_signer_role(), &f.signer));
    }
}

fn setter_calls(env: &Env, f: &Fixture<'_>, caller: &Address) -> [(&'static str, Vec<Val>); 7] {
    [
        ("set_claim_cooldown", (caller, 172_800_u128).into_val(env)),
        ("set_required_signers", (caller, 2_u128).into_val(env)),
        ("add_currency_limit", (caller, &f.validator, u(env, 100)).into_val(env)),
        ("set_currency_token_limit", (caller, &f.accepted_currency, u(env, 100)).into_val(env)),
        ("add_no_claim_fee_address", (caller, &f.pauser).into_val(env)),
        ("remove_no_claim_fee_address", (caller, &f.signer).into_val(env)),
        ("set_kyc_validator", (caller, None::<Address>).into_val(env)),
    ]
}

#[test]
fn both_admins_can_use_all_seven_setters_with_exact_auth() {
    for second in [false, true] {
        for operation in 0..7 {
            let env = auth::test_env();
            let f = fixture(&env);
            let access = FuulAccessControlClient::new(&env, &f.client.address);
            let next = Address::generate(&env);
            let outsider = Address::generate(&env);
            env.mock_all_auths();
            access.grant_role(&access.default_admin_role(), &next, &f.admin);
            f.client.add_no_claim_fee_address(&f.admin, &f.signer);
            let caller = if second { &next } else { &f.admin };
            let (name, args) = setter_calls(&env, &f, caller)[operation].clone();
            for wrong in [false, true] {
                env.mock_auths(&[]);
                if wrong {
                    auth::authorize(&env, &f.client.address, &outsider, name, args.clone());
                }
                let before = state(&env, &f.client.address);
                assert_eq!(
                    env.try_invoke_contract::<(), Error>(
                        &f.client.address,
                        &Symbol::new(&env, name),
                        args.clone()
                    ),
                    Err(Ok(auth::native_auth_error()))
                );
                auth::assert_auth_failure(&env, 0);
                assert!(env.events().all().events().is_empty());
                assert_eq!(state(&env, &f.client.address), before);
            }
            let (_, outsider_args) = setter_calls(&env, &f, &outsider)[operation].clone();
            auth::authorize(&env, &f.client.address, &outsider, name, outsider_args.clone());
            let before = state(&env, &f.client.address);
            assert_eq!(
                env.try_invoke_contract::<(), Error>(
                    &f.client.address,
                    &Symbol::new(&env, name),
                    outsider_args
                ),
                Err(Ok(Error::from_contract_error(2000)))
            );
            assert!(env.events().all().events().is_empty());
            assert_eq!(state(&env, &f.client.address), before);
            auth::authorize(&env, &f.client.address, caller, name, args.clone());
            assert_eq!(
                env.try_invoke_contract::<(), Error>(
                    &f.client.address,
                    &Symbol::new(&env, name),
                    args
                ),
                Ok(Ok(()))
            );
            assert_eq!(env.events().all().events().len(), 1);
            assert_eq!(
                env.auths().iter().map(|(a, _)| a.clone()).collect::<std::vec::Vec<_>>(),
                std::vec![caller.clone()]
            );
            match operation {
                0 => assert_eq!(f.client.claim_cooldown(), 172_800),
                1 => assert_eq!(f.client.required_signers(), 2),
                2 => {
                    assert_eq!(
                        f.client.currency_limits(&f.validator).claim_limit_per_cooldown,
                        u(&env, 100)
                    )
                }
                3 => assert_eq!(
                    f.client.currency_limits(&f.accepted_currency).claim_limit_per_cooldown,
                    u(&env, 100)
                ),
                4 => assert!(f.client.no_claim_fee_addresses(&f.pauser)),
                5 => assert!(!f.client.no_claim_fee_addresses(&f.signer)),
                _ => assert_eq!(f.client.kyc_validator(), None),
            }
            assert_eq!(access.get_role_member_count(&access.default_admin_role()), 2);
        }
    }
}

#[test]
fn legacy_admin_and_role_admin_keys_do_not_confer_authority() {
    let env = auth::test_env();
    let f = fixture(&env);
    let outsider = Address::generate(&env);
    let role = f.client.claim_signer_role();
    env.as_contract(&f.client.address, || {
        env.storage().instance().set(&OzKey::Admin, &outsider);
        env.storage().persistent().set(&OzKey::RoleAdmin(role.clone()), &role);
    });
    let calls: [(&str, Vec<Val>, &Address); 2] = [
        ("set_required_signers", (&outsider, 2_u128).into_val(&env), &outsider),
        ("grant_role", (&role, &outsider, &f.signer).into_val(&env), &f.signer),
    ];
    for (name, args, caller) in calls {
        auth::authorize(&env, &f.client.address, caller, name, args.clone());
        let before = state(&env, &f.client.address);
        assert_eq!(
            env.try_invoke_contract::<(), Error>(&f.client.address, &Symbol::new(&env, name), args),
            Err(Ok(Error::from_contract_error(2000)))
        );
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
    }
}
