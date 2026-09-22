use crate::test::*;
mod cooldown_domain;
mod security;

#[test]
fn admin_adds_and_updates_a_currency_limit_with_exact_events() {
    let env = Env::default();
    let fixture = fixture(&env);
    let token = Address::generate(&env);
    env.mock_all_auths();

    fixture.client.add_currency_limit(&fixture.admin, &token, &u(&env, 500_000));
    assert_eq!(
        env.events().all(),
        std::vec![TokenLimitAdded { token: token.clone(), limit: u(&env, 500_000) }
            .to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.currency_limits(&token).claim_limit_per_cooldown, u(&env, 500_000));

    fixture.client.set_currency_token_limit(&fixture.admin, &token, &u(&env, 750_000));
    assert_eq!(
        env.events().all(),
        std::vec![TokenLimitUpdated { token: token.clone(), limit: u(&env, 750_000) }
            .to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.currency_limits(&token).claim_limit_per_cooldown, u(&env, 750_000));
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn add_currency_rejects_zero_limit() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.add_currency_limit(&fixture.admin, &Address::generate(&env), &u(&env, 0));
}

#[test]
#[should_panic(expected = "Error(Contract, #6302)")]
fn add_currency_rejects_an_existing_currency() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.add_currency_limit(
        &fixture.admin,
        &fixture.accepted_currency,
        &u(&env, 500_000),
    );
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn add_currency_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.add_currency_limit(&fixture.admin, &Address::generate(&env), &u(&env, 500_000));
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn update_currency_rejects_a_missing_currency() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_currency_token_limit(
        &fixture.admin,
        &Address::generate(&env),
        &u(&env, 500_000),
    );
}

#[test]
fn update_currency_rejects_zero_and_unchanged_limits() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_currency_token_limit(
        &fixture.admin,
        &fixture.accepted_currency,
        &u(&env, 0),
    );
    assert_eq!(
        env.events().all(),
        std::vec![TokenLimitUpdated {
            token: fixture.accepted_currency.clone(),
            limit: u(&env, 0)
        }
        .to_xdr(&env, &fixture.client.address)]
    );
    for limit in [0, 1] {
        assert_eq!(
            fixture.client.try_set_currency_token_limit(
                &fixture.admin,
                &fixture.accepted_currency,
                &u(&env, limit)
            ),
            Err(Ok(Error::from_contract_error(6300)))
        );
        assert!(env.events().all().events().is_empty());
    }
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn update_currency_rejects_its_current_limit() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_currency_token_limit(
        &fixture.admin,
        &fixture.accepted_currency,
        &u(&env, 1_000_000_000_000_i128),
    );
}

#[test]
fn update_currency_accepts_a_limit_below_cumulative_claims() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    env.as_contract(&fixture.client.address, || {
        storage::set_currency_limit(
            &env,
            &fixture.accepted_currency,
            &CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, 1_000),
                cumulative_claim_per_cooldown: u(&env, 800),
                claim_cooldown_period_started: 1_000_000,
            },
        );
    });

    fixture.client.set_currency_token_limit(
        &fixture.admin,
        &fixture.accepted_currency,
        &u(&env, 799),
    );
    assert_eq!(
        env.events().all(),
        std::vec![TokenLimitUpdated {
            token: fixture.accepted_currency.clone(),
            limit: u(&env, 799)
        }
        .to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(
        fixture.client.currency_limits(&fixture.accepted_currency),
        CurrencyTokenLimit {
            claim_limit_per_cooldown: u(&env, 799),
            cumulative_claim_per_cooldown: u(&env, 800),
            claim_cooldown_period_started: 1_000_000,
        }
    );
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn update_currency_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.set_currency_token_limit(
        &fixture.admin,
        &fixture.accepted_currency,
        &u(&env, 500_000),
    );
}
