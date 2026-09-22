use crate::test::*;
use crate::test::{roles_pause_helpers::test_env, security_helpers::state};

#[test]
fn currency_add_accepts_one_and_i128_max_with_complete_window_state() {
    for limit in [1, i128::MAX] {
        let env = test_env();
        let f = fixture(&env);
        let token = Address::generate(&env);
        env.mock_all_auths();
        f.client.add_currency_limit(&f.admin, &token, &u(&env, limit));
        assert_eq!(
            env.events().all(),
            std::vec![TokenLimitAdded { token: token.clone(), limit: u(&env, limit) }
                .to_xdr(&env, &f.client.address)]
        );
        assert_eq!(
            f.client.currency_limits(&token),
            CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, limit),
                cumulative_claim_per_cooldown: u(&env, 0),
                claim_cooldown_period_started: 1_000_000,
            }
        );
    }
}

#[test]
fn lowered_currency_limit_preserves_active_and_expired_window_state() {
    for timestamp in [1_000_001, 1_086_400] {
        let env = test_env();
        let f = claim_fixture(&env);
        let project = register_mock_project(&env, &f.admin, 0);
        f.client.set_currency_token_limit(&f.admin, &f.currency, &u(&env, 1_000));
        f.client.claim(&f.caller, &vec![&env, claim_check(&env, &f, &project.address, 800, 1)]);
        f.client.set_currency_token_limit(&f.admin, &f.currency, &u(&env, 800));
        assert_eq!(
            f.client.currency_limits(&f.currency),
            CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, 800),
                cumulative_claim_per_cooldown: u(&env, 800),
                claim_cooldown_period_started: 1_000_000,
            }
        );
        env.ledger().with_mut(|l| l.timestamp = timestamp);
        f.client.set_currency_token_limit(&f.admin, &f.currency, &u(&env, 500));
        assert_eq!(
            env.events().all(),
            std::vec![TokenLimitUpdated { token: f.currency.clone(), limit: u(&env, 500) }
                .to_xdr(&env, &f.client.address)]
        );
        assert_eq!(
            f.client.currency_limits(&f.currency),
            CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, 500),
                cumulative_claim_per_cooldown: u(&env, 800),
                claim_cooldown_period_started: 1_000_000,
            }
        );
        let amount = if timestamp == 1_000_001 { 1 } else { 100 };
        let checks = vec![&env, claim_check(&env, &f, &project.address, amount, 2)];
        let before = state(&env, &f.client.address);
        if timestamp == 1_000_001 {
            assert_eq!(
                f.client.try_claim(&f.caller, &checks),
                Err(Ok(Error::from_contract_error(6304)))
            );
            assert!(env.events().all().events().is_empty());
            assert_eq!(state(&env, &f.client.address), before);
            assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 800));
        } else {
            f.client.claim(&f.caller, &checks);
            assert_eq!(
                f.client.currency_limits(&f.currency),
                CurrencyTokenLimit {
                    claim_limit_per_cooldown: u(&env, 500),
                    cumulative_claim_per_cooldown: u(&env, 100),
                    claim_cooldown_period_started: timestamp,
                }
            );
            assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 900));
        }
    }
}

#[test]
fn zero_limit_readdition_resets_only_the_currency_window_and_preserves_project_proofs() {
    for timestamp in [1_000_001, 1_086_400] {
        let env = test_env();
        let f = claim_fixture(&env);
        let factory = env.register(
            FuulFactory,
            (
                f.admin.clone(),
                f.client.address.clone(),
                Address::generate(&env),
                env.deployer().upload_contract_wasm(PROJECT_WASM),
            ),
        );
        let factory = FuulFactoryClient::new(&env, &factory);
        let project =
            factory.create_fuul_project(&f.admin, &String::from_str(&env, "ipfs://limits"), &false);
        factory.set_default_project_claim_fee(&f.admin, &0);
        StellarAssetClient::new(&env, &f.currency).mint(&project, &2_000);
        let project_client = FuulProjectClient::new(&env, &project);
        f.client.set_currency_token_limit(&f.admin, &f.currency, &u(&env, 1_000));
        let first = claim_check(&env, &f, &project, 800, 71);
        f.client.claim(&f.caller, &vec![&env, first.clone()]);
        env.ledger().with_mut(|l| l.timestamp = timestamp);
        let project_before = state(&env, &project);
        let balances_before = [
            TokenClient::new(&env, &f.currency).balance(&project),
            TokenClient::new(&env, &f.currency).balance(&f.recipient),
        ];
        f.client.set_currency_token_limit(&f.admin, &f.currency, &u(&env, 0));
        assert_eq!(
            env.events().all(),
            std::vec![TokenLimitUpdated { token: f.currency.clone(), limit: u(&env, 0) }
                .to_xdr(&env, &f.client.address)]
        );
        assert_eq!(
            f.client.currency_limits(&f.currency),
            CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, 0),
                cumulative_claim_per_cooldown: u(&env, 800),
                claim_cooldown_period_started: 1_000_000,
            }
        );
        for limit in [-1, 0, 500] {
            let before = state(&env, &f.client.address);
            let encoded: Val =
                if limit < 0 { (-1_i128).into_val(&env) } else { u(&env, limit).into_val(&env) };
            assert_eq!(
                env.try_invoke_contract::<(), Error>(
                    &f.client.address,
                    &Symbol::new(&env, "set_currency_token_limit"),
                    (&f.admin, &f.currency, encoded).into_val(&env)
                ),
                Err(Ok(if limit < 0 {
                    roles_pause_helpers::native_auth_error()
                } else {
                    Error::from_contract_error(6300)
                }))
            );
            assert!(env.events().all().events().is_empty());
            assert_eq!(state(&env, &f.client.address), before);
        }
        f.client.add_currency_limit(&f.admin, &f.currency, &u(&env, 500));
        assert_eq!(
            env.events().all(),
            std::vec![TokenLimitAdded { token: f.currency.clone(), limit: u(&env, 500) }
                .to_xdr(&env, &f.client.address)]
        );
        assert_eq!(
            f.client.currency_limits(&f.currency),
            CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, 500),
                cumulative_claim_per_cooldown: u(&env, 0),
                claim_cooldown_period_started: timestamp,
            }
        );
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 800));
        assert_eq!(state(&env, &project), project_before);
        assert!(project_client.claimed_proofs(&first.proof));
        assert_eq!(
            [
                TokenClient::new(&env, &f.currency).balance(&project),
                TokenClient::new(&env, &f.currency).balance(&f.recipient)
            ],
            balances_before
        );
        let mut replay = first;
        replay.amount = 100;
        replay.deadline = u(&env, timestamp + 300);
        let before = state(&env, &f.client.address);
        assert_eq!(
            f.client.try_claim(&f.caller, &vec![&env, replay]),
            Err(Ok(Error::from_contract_error(6102)))
        );
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
        assert_eq!(state(&env, &project), project_before);
        f.client.claim(&f.caller, &vec![&env, claim_check(&env, &f, &project, 100, 72)]);
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 900));
        assert_eq!(
            f.client.currency_limits(&f.currency).cumulative_claim_per_cooldown,
            u(&env, 100)
        );
        assert!(project_client.claimed_proofs(&BytesN::from_array(&env, &[72; 32])));
    }
}

#[test]
fn zero_limit_claims_follow_the_cumulative_window_before_and_at_expiry() {
    for timestamp in [1_000_001, 1_086_400] {
        let env = test_env();
        let f = claim_fixture(&env);
        let project = register_mock_project(&env, &f.admin, 0);
        f.client.claim(&f.caller, &vec![&env, claim_check(&env, &f, &project.address, 800, 73)]);
        f.client.set_currency_token_limit(&f.admin, &f.currency, &u(&env, 0));
        env.ledger().with_mut(|l| l.timestamp = timestamp);
        let before = state(&env, &f.client.address);
        assert_eq!(
            f.client
                .try_claim(&f.caller, &vec![&env, claim_check(&env, &f, &project.address, 1, 74)]),
            Err(Ok(Error::from_contract_error(6304)))
        );
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
        let checks = vec![&env, claim_check(&env, &f, &project.address, 0, 74)];
        if timestamp == 1_000_001 {
            assert_eq!(
                f.client.try_claim(&f.caller, &checks),
                Err(Ok(Error::from_contract_error(6304)))
            );
            assert!(env.events().all().events().is_empty());
            assert_eq!(state(&env, &f.client.address), before);
        } else {
            f.client.claim(&f.caller, &checks);
            assert_eq!(env.events().all().filter_by_contract(&f.client.address).events().len(), 1);
            assert_eq!(
                f.client.currency_limits(&f.currency),
                CurrencyTokenLimit {
                    claim_limit_per_cooldown: u(&env, 0),
                    cumulative_claim_per_cooldown: u(&env, 0),
                    claim_cooldown_period_started: timestamp,
                }
            );
        }
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 800));
    }
}

#[test]
fn mutable_cooldown_increase_and_decrease_use_the_existing_window_and_equality_resets() {
    for (initial, updated) in [(86_400, 172_800), (172_800, 86_400)] {
        let env = test_env();
        let f = claim_fixture(&env);
        let project = register_mock_project(&env, &f.admin, 0);
        if initial != 86_400 {
            f.client.set_claim_cooldown(&f.admin, &initial);
        }
        f.client.set_currency_token_limit(&f.admin, &f.currency, &u(&env, 1_000));
        f.client.claim(&f.caller, &vec![&env, claim_check(&env, &f, &project.address, 800, 1)]);
        f.client.set_claim_cooldown(&f.admin, &updated);
        env.ledger().with_mut(|l| l.timestamp = 1_000_000 + u64::try_from(updated).unwrap() - 1);
        let before = state(&env, &f.client.address);
        assert_eq!(
            f.client
                .try_claim(&f.caller, &vec![&env, claim_check(&env, &f, &project.address, 201, 2)]),
            Err(Ok(Error::from_contract_error(6304)))
        );
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
        env.ledger().with_mut(|l| l.timestamp += 1);
        f.client.claim(&f.caller, &vec![&env, claim_check(&env, &f, &project.address, 201, 2)]);
        assert_eq!(
            f.client.currency_limits(&f.currency),
            CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, 1_000),
                cumulative_claim_per_cooldown: u(&env, 201),
                claim_cooldown_period_started: 1_000_000 + u64::try_from(updated).unwrap(),
            }
        );
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 1_001));
    }
}

#[test]
fn maximum_cooldown_keeps_an_otherwise_valid_under_limit_claim_available() {
    let env = test_env();
    let f = claim_fixture(&env);
    let project = register_mock_project(&env, &f.admin, 0);
    let checks = vec![&env, claim_check(&env, &f, &project.address, 1, 9)];
    f.client.set_claim_cooldown(&f.admin, &u128::MAX);
    assert_eq!(f.client.try_claim(&f.caller, &checks), Ok(Ok(())));
    assert_eq!(f.client.claim_cooldown(), u128::MAX);
    assert_eq!(project.last_recipient(), Some(f.recipient.clone()));
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 1));
    assert_eq!(
        f.client.currency_limits(&f.currency),
        CurrencyTokenLimit {
            claim_limit_per_cooldown: u(&env, 1_000_000_000_000_i128),
            cumulative_claim_per_cooldown: u(&env, 1),
            claim_cooldown_period_started: 1_000_000,
        }
    );
    // Use a fresh proof for the normal-cooldown control, as a real Project requires.
    f.client.set_claim_cooldown(&f.admin, &86_400);
    let next_checks = vec![&env, claim_check(&env, &f, &project.address, 1, 10)];
    f.client.claim(&f.caller, &next_checks);
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 2));
    assert_eq!(f.client.currency_limits(&f.currency).cumulative_claim_per_cooldown, u(&env, 2));
}
