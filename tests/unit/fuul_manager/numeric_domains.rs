use crate::test::{roles_pause_helpers::test_env, *};
use soroban_sdk::{Map, TryFromVal, U256};

fn maximum(e: &Env) -> U256 {
    U256::from_parts(e, u64::MAX, u64::MAX, u64::MAX, u64::MAX)
}

#[test]
fn cooldown_accepts_full_u128_without_summing_it_into_the_ledger_clock() {
    let e = test_env();
    let f = claim_fixture(&e);
    let project = register_mock_project(&e, &f.admin, 0);
    for period in [u128::from(u64::MAX) + 1, u128::MAX] {
        let result = e.try_invoke_contract::<(), Error>(
            &f.client.address,
            &Symbol::new(&e, "set_claim_cooldown"),
            (&f.admin, period).into_val(&e),
        );
        assert_eq!(result, Ok(Ok(())));
        let check = claim_check(&e, &f, &project.address, 10, (period & 255) as u8);
        f.client.claim(&f.caller, &vec![&e, check]);
        let actual: u128 =
            e.invoke_contract(&f.client.address, &Symbol::new(&e, "claim_cooldown"), vec![&e]);
        assert_eq!(actual, period);
    }
}

#[test]
fn quorum_above_u32_is_valid_configuration_but_does_not_bypass_signer_count() {
    let e = test_env();
    let f = claim_fixture(&e);
    let project = register_mock_project(&e, &f.admin, 0);
    for quorum in [u128::from(u32::MAX) + 1, (1_u128 << 96) - 1] {
        assert_eq!(
            e.try_invoke_contract::<(), Error>(
                &f.client.address,
                &Symbol::new(&e, "set_required_signers"),
                (&f.admin, quorum).into_val(&e)
            ),
            Ok(Ok(()))
        );
        assert_eq!(
            f.client.try_claim(&f.caller, &vec![&e, claim_check(&e, &f, &project.address, 1, 121)]),
            Err(Ok(Error::from_contract_error(6306)))
        );
    }
}

#[test]
fn currency_limit_accepts_u256_maximum_without_narrowing() {
    let e = test_env();
    let f = claim_fixture(&e);
    assert_eq!(
        e.try_invoke_contract::<(), Error>(
            &f.client.address,
            &Symbol::new(&e, "set_currency_token_limit"),
            (&f.admin, &f.currency, maximum(&e)).into_val(&e)
        ),
        Ok(Ok(()))
    );
    let value: Val = e.invoke_contract(
        &f.client.address,
        &Symbol::new(&e, "currency_limits"),
        (&f.currency,).into_val(&e),
    );
    let fields = Map::<Symbol, Val>::try_from_val(&e, &value).unwrap();
    assert_eq!(
        U256::try_from_val(&e, &fields.get(Symbol::new(&e, "claim_limit_per_cooldown")).unwrap())
            .unwrap(),
        maximum(&e)
    );
}

#[test]
fn constructor_accepts_full_u256_limit_in_native_and_wasm() {
    for compiled in [false, true] {
        let e = test_env();
        let input = constructor_helpers::Bootstrap::new(&e);
        let mut args = input.args(&e);
        args.set(8, maximum(&e).into_val(&e));
        let id = if compiled {
            e.register(constructor_helpers::MANAGER_WASM, args)
        } else {
            e.register(FuulManager, args)
        };
        let client = FuulManagerClient::new(&e, &id);
        assert_eq!(client.currency_limits(&input.accepted).claim_limit_per_cooldown, maximum(&e));
        assert_eq!(
            client.currency_limits(&input.native).claim_limit_per_cooldown,
            u(&e, 1_000_000_000_000_i128)
        );
    }
}

#[test]
fn claim_accepts_u256_deadline_and_opaque_fungible_token_id() {
    let e = test_env();
    let f = claim_fixture(&e);
    let project = register_mock_project(&e, &f.admin, 0);
    let check = claim_check(&e, &f, &project.address, 10, 122);
    let encoded: Val = check.into_val(&e);
    let mut fields = Map::<Symbol, Val>::try_from_val(&e, &encoded).unwrap();
    fields.set(Symbol::new(&e, "deadline"), maximum(&e).into_val(&e));
    fields.set(Symbol::new(&e, "token_id"), maximum(&e).into_val(&e));
    assert_eq!(
        e.try_invoke_contract::<(), Error>(
            &f.client.address,
            &Symbol::new(&e, "claim"),
            (&f.caller, vec![&e, fields]).into_val(&e)
        ),
        Ok(Ok(()))
    );
    assert_eq!(project.last_recipient(), Some(f.recipient));
}

#[test]
fn quorum_above_uint96_rejects_after_admin_auth_without_changes() {
    let e = test_env();
    let f = claim_fixture(&e);
    for authorized in [false, true] {
        if authorized {
            e.mock_all_auths();
        } else {
            e.set_auths(&[]);
        }
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            f.client.try_set_required_signers(&f.admin, &(1_u128 << 96)),
            Err(Ok(if authorized {
                Error::from_contract_error(6300)
            } else {
                roles_pause_helpers::native_auth_error()
            }))
        );
        assert!(e.events().all().events().is_empty());
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
    }
}

#[test]
fn elapsed_window_handles_full_clock_and_period_boundaries() {
    for (start, now, period, active) in [
        (1_u64, u64::MAX, u128::MAX, true),
        (1, u64::MAX, u128::from(u64::MAX), true),
        (1, u64::MAX, u128::from(u64::MAX) - 1, false),
        (1, 0, u128::MAX, true),
    ] {
        let e = test_env();
        let f = claim_fixture(&e);
        let project = register_mock_project(&e, &f.admin, 0);
        f.client.set_claim_cooldown(&f.admin, &period);
        e.as_contract(&f.client.address, || {
            storage::set_currency_limit(
                &e,
                &f.currency,
                &CurrencyTokenLimit {
                    claim_limit_per_cooldown: u(&e, 10),
                    cumulative_claim_per_cooldown: u(&e, 1),
                    claim_cooldown_period_started: start,
                },
            )
        });
        let mut check = claim_check(&e, &f, &project.address, 2, 123);
        check.deadline = maximum(&e);
        e.ledger().with_mut(|l| l.timestamp = now);
        f.client.claim(&f.caller, &vec![&e, check]);
        let window = f.client.currency_limits(&f.currency);
        assert_eq!(window.cumulative_claim_per_cooldown, u(&e, if active { 3 } else { 2 }));
        assert_eq!(window.claim_cooldown_period_started, if active { start } else { now });
    }
}

#[test]
fn deadline_equality_and_expiry_at_maximum_ledger_time_are_unsigned() {
    let e = test_env();
    let f = claim_fixture(&e);
    let project = register_mock_project(&e, &f.admin, 0);
    let mut check = claim_check(&e, &f, &project.address, 1, 124);
    check.deadline = u(&e, u64::MAX - 1);
    e.ledger().with_mut(|l| l.timestamp = u64::MAX);
    let before = e.to_ledger_snapshot().ledger_entries;
    assert_eq!(
        f.client.try_claim(&f.caller, &vec![&e, check.clone()]),
        Err(Ok(Error::from_contract_error(6305)))
    );
    assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
    assert!(e.events().all().events().is_empty());
    check.deadline = u(&e, u64::MAX);
    f.client.claim(&f.caller, &vec![&e, check]);
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&e, 1));
}

#[test]
fn real_transfers_can_accumulate_beyond_i128_and_c2_readdition_keeps_totals_and_proofs() {
    for compiled in [false, true] {
        let e = test_env();
        let mut f = claim_fixture(&e);
        if compiled {
            let id = e.register(
                constructor_helpers::MANAGER_WASM,
                (
                    &f.admin,
                    &f.admin,
                    &f.admin,
                    1_u128,
                    vec![&e, f.signer.clone()],
                    &f.currency,
                    &f.native_asset,
                    None::<Address>,
                    u(&e, 1_000_000_000_000_i128),
                ),
            );
            f.client = FuulManagerClient::new(&e, &id);
        }
        let factory_id = e.register(
            FuulFactory,
            (
                &f.admin,
                &f.client.address,
                &f.admin,
                e.deployer().upload_contract_wasm(PROJECT_WASM),
            ),
        );
        let factory = FuulFactoryClient::new(&e, &factory_id);
        factory.set_default_project_claim_fee(&f.admin, &0);
        let project = factory.create_fuul_project(
            &f.admin,
            &String::from_str(&e, "ipfs://wide-accounting"),
            &false,
        );
        f.client.set_currency_token_limit(&f.admin, &f.currency, &maximum(&e));
        StellarAssetClient::new(&e, &f.currency).mint(&project, &i128::MAX);
        let token = TokenClient::new(&e, &f.currency);
        let first = claim_check(&e, &f, &project, i128::MAX, 125);
        let second = claim_check(&e, &f, &project, i128::MAX, 126);
        let third = claim_check(&e, &f, &project, i128::MAX, 127);
        f.client.claim(&f.caller, &vec![&e, first.clone()]);
        token.transfer(&f.recipient, MuxedAddress::from(&project), &i128::MAX);
        f.client.claim(&f.caller, &vec![&e, second.clone()]);
        let twice = U256::from_parts(&e, 0, 0, u64::MAX, u64::MAX - 1);
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), twice);
        token.transfer(&f.recipient, MuxedAddress::from(&project), &i128::MAX);
        f.client.claim(&f.caller, &vec![&e, third.clone()]);
        let expected = U256::from_parts(&e, 0, 1, u64::MAX >> 1, u64::MAX - 2);
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), expected);
        assert_eq!(f.client.currency_limits(&f.currency).cumulative_claim_per_cooldown, expected);
        assert_eq!(token.balance(&f.recipient), i128::MAX);
        f.client.set_currency_token_limit(&f.admin, &f.currency, &u(&e, 0));
        assert_eq!(
            f.client.try_set_currency_token_limit(&f.admin, &f.currency, &u(&e, 1)),
            Err(Ok(Error::from_contract_error(6300)))
        );
        f.client.add_currency_limit(&f.admin, &f.currency, &maximum(&e));
        assert_eq!(f.client.currency_limits(&f.currency).cumulative_claim_per_cooldown, u(&e, 0));
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), expected);
        for c in [&first, &second, &third] {
            assert!(FuulProjectClient::new(&e, &project).claimed_proofs(&c.proof));
        }
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            f.client.try_claim(&f.caller, &vec![&e, first]),
            Err(Ok(Error::from_contract_error(6102)))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
    }
}

#[test]
fn legacy_numeric_storage_is_not_silently_reinterpreted_as_the_new_schema() {
    for cooldown in [false, true] {
        let e = test_env();
        let f = claim_fixture(&e);
        e.as_contract(&f.client.address, || {
            if cooldown {
                e.storage().instance().set(&(Symbol::new(&e, "ClaimCooldown"),), &86_400_u64);
            } else {
                e.storage()
                    .persistent()
                    .set(&(Symbol::new(&e, "UserClaims"), &f.recipient, &f.currency), &1_i128);
            }
        });
        let before = e.to_ledger_snapshot().ledger_entries;
        let (method, args) = if cooldown {
            ("claim_cooldown", vec![&e])
        } else {
            ("users_claims", (&f.recipient, &f.currency).into_val(&e))
        };
        assert_eq!(
            e.try_invoke_contract::<(), Error>(&f.client.address, &Symbol::new(&e, method), args),
            Err(Ok(roles_pause_helpers::native_auth_error()))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
    }
}
