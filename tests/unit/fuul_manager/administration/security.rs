use crate::test::*;
use crate::test::{roles_pause_helpers as auth, security_helpers::state};

#[test]
fn seven_configuration_mutators_require_exact_admin_auth_and_work_while_paused() {
    for operation in 0..7 {
        for paused in [false, true] {
            let env = auth::test_env();
            let f = fixture(&env);
            let account = Address::generate(&env);
            let outsider = Address::generate(&env);
            if operation == 5 {
                env.mock_all_auths();
                f.client.add_no_claim_fee_address(&f.admin, &account);
            }
            if paused {
                auth::transition(&env, &f.client, &f.pauser, true);
            }
            let (name, mut args): (&str, Vec<Val>) = match operation {
                0 => ("set_claim_cooldown", (172_800_u128,).into_val(&env)),
                1 => ("set_required_signers", (2_u128,).into_val(&env)),
                2 => ("add_currency_limit", (&account, u(&env, 100)).into_val(&env)),
                3 => (
                    "set_currency_token_limit",
                    (&f.accepted_currency, u(&env, 100)).into_val(&env),
                ),
                4 => ("add_no_claim_fee_address", (&account,).into_val(&env)),
                5 => ("remove_no_claim_fee_address", (&account,).into_val(&env)),
                _ => ("set_kyc_validator", (None::<Address>,).into_val(&env)),
            };
            args.push_front(f.admin.clone().into_val(&env));
            for third_party in [false, true] {
                env.mock_auths(&[]);
                if third_party {
                    auth::authorize(&env, &f.client.address, &outsider, name, args.clone());
                }
                let before = state(&env, &f.client.address);
                let result = env.try_invoke_contract::<(), Error>(
                    &f.client.address,
                    &Symbol::new(&env, name),
                    args.clone(),
                );
                assert_eq!(result, Err(Ok(auth::native_auth_error())), "{name}");
                // A new top-level invocation replaces the previous diagnostic list.
                auth::assert_auth_failure(&env, 0);
                assert!(env.events().all().events().is_empty());
                assert_eq!(state(&env, &f.client.address), before, "{name}");
            }
            auth::authorize(&env, &f.client.address, &f.admin, name, args.clone());
            env.invoke_contract::<()>(&f.client.address, &Symbol::new(&env, name), args);
            let events = env.events().all();
            let expected = match operation {
                0 => ClaimCooldownUpdated { period: 172_800 }.to_xdr(&env, &f.client.address),
                1 => RequiredSignersUpdated { value: 2 }.to_xdr(&env, &f.client.address),
                2 => TokenLimitAdded { token: account.clone(), limit: u(&env, 100) }
                    .to_xdr(&env, &f.client.address),
                3 => TokenLimitUpdated { token: f.accepted_currency.clone(), limit: u(&env, 100) }
                    .to_xdr(&env, &f.client.address),
                4 => NoClaimFeeAddressAdded { account: account.clone() }
                    .to_xdr(&env, &f.client.address),
                5 => NoClaimFeeAddressRemoved { account: account.clone() }
                    .to_xdr(&env, &f.client.address),
                _ => KycValidatorUpdated { validator: None }.to_xdr(&env, &f.client.address),
            };
            assert_eq!(events, std::vec![expected]);
            match operation {
                0 => assert_eq!(f.client.claim_cooldown(), 172_800),
                1 => assert_eq!(f.client.required_signers(), 2),
                2 => assert_eq!(
                    f.client.currency_limits(&account).claim_limit_per_cooldown,
                    u(&env, 100)
                ),
                3 => assert_eq!(
                    f.client.currency_limits(&f.accepted_currency).claim_limit_per_cooldown,
                    u(&env, 100)
                ),
                4 => assert!(f.client.no_claim_fee_addresses(&account)),
                5 => assert!(!f.client.no_claim_fee_addresses(&account)),
                _ => assert_eq!(f.client.kyc_validator(), None),
            }
            assert_eq!(f.client.paused(), paused);
        }
    }
}

#[test]
fn invalid_configuration_preserves_manager_entries_ttls_and_events() {
    let env = auth::test_env();
    env.ledger().with_mut(|l| l.min_persistent_entry_ttl = fuul_core::INSTANCE_EXTEND_AMOUNT + 1);
    let f = fixture(&env);
    let absent = Address::generate(&env);
    env.mock_all_auths();
    f.client.add_no_claim_fee_address(&f.admin, &f.signer);
    // Rejected getters/writes would extend low TTL unless their side effects roll back.
    env.ledger().with_mut(|l| {
        l.sequence_number +=
            fuul_core::INSTANCE_EXTEND_AMOUNT - fuul_core::INSTANCE_TTL_THRESHOLD + 1;
    });
    let cases: [(&str, Vec<Val>, u32); 12] = [
        ("set_claim_cooldown", (0_u128,).into_val(&env), 6300),
        ("set_claim_cooldown", (86_400_u128,).into_val(&env), 6300),
        ("set_required_signers", (0_u128,).into_val(&env), 6300),
        ("set_required_signers", (1_u128,).into_val(&env), 6300),
        ("add_currency_limit", (&absent, -1_i128).into_val(&env), 0),
        ("add_currency_limit", (&absent, u(&env, 0)).into_val(&env), 6300),
        ("add_currency_limit", (&f.accepted_currency, u(&env, 1)).into_val(&env), 6302),
        ("set_currency_token_limit", (&absent, u(&env, 1)).into_val(&env), 6300),
        ("set_currency_token_limit", (&f.accepted_currency, -1_i128).into_val(&env), 0),
        (
            "set_currency_token_limit",
            (&f.accepted_currency, u(&env, 1_000_000_000_000_i128)).into_val(&env),
            6300,
        ),
        ("add_no_claim_fee_address", (&f.signer,).into_val(&env), 6300),
        ("remove_no_claim_fee_address", (&absent,).into_val(&env), 6300),
    ];
    for (name, mut args, code) in cases {
        args.push_front(f.admin.clone().into_val(&env));
        auth::authorize(&env, &f.client.address, &f.admin, name, args.clone());
        let before = state(&env, &f.client.address);
        let result =
            env.try_invoke_contract::<(), Error>(&f.client.address, &Symbol::new(&env, name), args);
        assert_eq!(
            result,
            Err(Ok(if code == 0 {
                auth::native_auth_error()
            } else {
                Error::from_contract_error(code)
            })),
            "{name}"
        );
        if code == 0 {
            constructor_helpers::assert_diagnostic(
                &env,
                soroban_sdk::xdr::ScError::WasmVm(ScErrorCode::InvalidAction),
            );
        }
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before, "{name}");
    }
}
