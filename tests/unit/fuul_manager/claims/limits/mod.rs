use crate::test::*;

#[test]
fn prepared_claim_expires_after_the_ledger_clock_advances() {
    let env = Env::default();
    let f = claim_fixture(&env);
    let p = register_mock_project(&env, &Address::generate(&env), 0);
    let mut check = claim_check(&env, &f, &p.address, 10, 91);
    check.deadline = u(&env, env.ledger().timestamp() + 60);
    let checks = vec![&env, check];
    security_helpers::authorize_claims(&env, &f.client.address, &f.caller, &checks, &[]);
    env.ledger().with_mut(|l| l.timestamp += 120);
    let before = env.to_ledger_snapshot().ledger_entries;
    assert_eq!(f.client.try_claim(&f.caller, &checks), Err(Ok(Error::from_contract_error(6305))));
    assert_eq!(env.to_ledger_snapshot().ledger_entries, before);
    assert!(env.events().all().events().is_empty());
}

#[test]
fn cooldown_boundary_resets_the_cumulative_limit() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    fixture.client.set_currency_token_limit(&fixture.admin, &fixture.currency, &u(&env, 100));
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 90, 7)]);
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_086_400);

    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 100, 8)]);

    assert_eq!(
        fixture.client.currency_limits(&fixture.currency),
        CurrencyTokenLimit {
            claim_limit_per_cooldown: u(&env, 100),
            cumulative_claim_per_cooldown: u(&env, 100),
            claim_cooldown_period_started: 1_086_400,
        }
    );
}

#[test]
fn deadline_equal_to_the_current_ledger_time_is_valid() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    let mut check = claim_check(&env, &fixture, &project.address, 10, 9);
    check.deadline = u(&env, env.ledger().timestamp());

    fixture.client.claim(&fixture.caller, &vec![&env, check]);

    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 10));
}

#[test]
#[should_panic(expected = "Error(Contract, #6304)")]
fn claim_rejects_an_amount_above_the_currency_limit() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    fixture.client.set_currency_token_limit(&fixture.admin, &fixture.currency, &u(&env, 99));
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);

    fixture.client.claim(
        &fixture.caller,
        &vec![&env, claim_check(&env, &fixture, &project.address, 100, 10)],
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6304)")]
fn claim_rejects_a_cumulative_amount_above_the_currency_limit() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    fixture.client.set_currency_token_limit(&fixture.admin, &fixture.currency, &u(&env, 100));
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 60, 11)]);

    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 41, 12)]);
}

#[test]
#[should_panic(expected = "Error(Contract, #6305)")]
fn claim_rejects_an_expired_deadline() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    let mut check = claim_check(&env, &fixture, &project.address, 10, 13);
    check.deadline = u(&env, env.ledger().timestamp() - 1);

    fixture.client.claim(&fixture.caller, &vec![&env, check]);
}

#[test]
fn batch_accepts_distinct_unexpired_deadlines() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    let mut first = claim_check(&env, &fixture, &project.address, 10, 36);
    let mut second = claim_check(&env, &fixture, &project.address, 20, 37);
    first.deadline = u(&env, env.ledger().timestamp() + 1);
    second.deadline = u(&env, env.ledger().timestamp() + 1_000);

    fixture.client.claim(&fixture.caller, &vec![&env, first, second]);

    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 30));
}
