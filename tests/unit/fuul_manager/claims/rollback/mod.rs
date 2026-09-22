use crate::test::*;
mod reentry;

#[test]
fn failed_batch_rolls_back_limit_and_user_accounting() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, -1);
    let check = claim_check(&env, &fixture, &project.address, 55, 25);

    assert!(fixture.client.try_claim(&fixture.caller, &vec![&env, check]).is_err());
    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 0));
    assert_eq!(
        fixture.client.currency_limits(&fixture.currency).cumulative_claim_per_cooldown,
        u(&env, 0)
    );
}

#[test]
fn earlier_replay_failure_precedes_a_later_over_limit_check() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    fixture.client.set_currency_token_limit(&fixture.admin, &fixture.currency, &u(&env, 100));
    let replay_project = env.register(ReplayRejectingProject, ());
    let later_project = register_mock_project(&env, &Address::generate(&env), 0);
    let first = claim_check(&env, &fixture, &replay_project, 10, 51);
    let second = claim_check(&env, &fixture, &later_project.address, 101, 52);

    assert_eq!(
        fixture.client.try_claim(&fixture.caller, &vec![&env, first, second]),
        Err(Ok(Error::from_contract_error(6102)))
    );
    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 0));
    assert_eq!(
        fixture.client.currency_limits(&fixture.currency).cumulative_claim_per_cooldown,
        u(&env, 0)
    );
    assert_eq!(later_project.last_recipient(), None);
}

#[test]
fn earlier_kyc_failure_precedes_a_later_expired_check() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let kyc_project = env.register(KycRejectingProject, ());
    let later_project = register_mock_project(&env, &Address::generate(&env), 0);
    let first = claim_check(&env, &fixture, &kyc_project, 10, 53);
    let mut second = claim_check(&env, &fixture, &later_project.address, 20, 54);
    second.deadline = u(&env, env.ledger().timestamp() - 1);

    assert_eq!(
        fixture.client.try_claim(&fixture.caller, &vec![&env, first, second]),
        Err(Ok(Error::from_contract_error(6103)))
    );
    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 0));
    assert_eq!(
        fixture.client.currency_limits(&fixture.currency).cumulative_claim_per_cooldown,
        u(&env, 0)
    );
    assert_eq!(later_project.last_recipient(), None);
}

#[test]
fn valid_first_claim_and_expired_second_claim_roll_back_the_complete_batch() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let first_project = register_mock_project(&env, &Address::generate(&env), 0);
    let second_project = register_mock_project(&env, &Address::generate(&env), 0);
    let first = claim_check(&env, &fixture, &first_project.address, 10, 55);
    let mut second = claim_check(&env, &fixture, &second_project.address, 20, 56);
    second.deadline = u(&env, env.ledger().timestamp() - 1);

    assert_eq!(
        fixture.client.try_claim(&fixture.caller, &vec![&env, first, second]),
        Err(Ok(Error::from_contract_error(6305)))
    );
    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 0));
    assert_eq!(
        fixture.client.currency_limits(&fixture.currency).cumulative_claim_per_cooldown,
        u(&env, 0)
    );
    assert_eq!(first_project.last_recipient(), None);
    assert_eq!(second_project.last_recipient(), None);
}

#[test]
fn project_callback_cannot_reenter_manager_claim() {
    reentry::assert_callback_rejection_and_control();
}
