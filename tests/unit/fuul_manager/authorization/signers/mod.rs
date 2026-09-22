use crate::test::*;

#[test]
#[should_panic(expected = "Error(Contract, #6306)")]
fn claim_rejects_fewer_signers_than_the_threshold() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    fixture.client.set_required_signers(&fixture.admin, &2);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);

    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 10, 14)]);
}

#[test]
fn claim_accepts_the_required_two_approved_signers() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let second_signer = Address::generate(&env);
    let access = FuulAccessControlClient::new(&env, &fixture.client.address);
    access.grant_role(&fixture.client.claim_signer_role(), &second_signer, &fixture.admin);
    fixture.client.set_required_signers(&fixture.admin, &2);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    let mut check = claim_check(&env, &fixture, &project.address, 10, 27);
    check.signers = vec![&env, fixture.signer.clone(), second_signer];

    fixture.client.claim(&fixture.caller, &vec![&env, check]);

    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 10));
}

#[test]
fn claim_accepts_more_approved_signers_than_required() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let second_signer = Address::generate(&env);
    let access = FuulAccessControlClient::new(&env, &fixture.client.address);
    access.grant_role(&fixture.client.claim_signer_role(), &second_signer, &fixture.admin);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    let mut check = claim_check(&env, &fixture, &project.address, 10, 28);
    check.signers = vec![&env, fixture.signer.clone(), second_signer];

    fixture.client.claim(&fixture.caller, &vec![&env, check]);

    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 10));
}

#[test]
#[should_panic(expected = "Error(Contract, #6307)")]
fn claim_rejects_a_signer_without_the_claim_signer_role() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    let mut check = claim_check(&env, &fixture, &project.address, 10, 15);
    check.signers = vec![&env, Address::generate(&env)];

    fixture.client.claim(&fixture.caller, &vec![&env, check]);
}

#[test]
fn invalid_signer_role_precedes_missing_smart_account_authorization() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let project = register_mock_project(&env, &Address::generate(&env), 0);
    let invalid_signer = Address::generate(&env);
    let mut check = claim_check(&env, &fixture, &project.address, 10, 63);
    check.signers = vec![&env, invalid_signer];
    let checks = vec![&env, check];

    let result = fixture
        .client
        .mock_auths(&[MockAuth {
            address: &fixture.caller,
            invoke: &MockAuthInvoke {
                contract: &fixture.client.address,
                fn_name: "claim",
                args: (&fixture.caller, &checks).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_claim(&fixture.caller, &checks);

    assert_eq!(result, Err(Ok(Error::from_contract_error(6307))));
    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 0));
    assert_eq!(
        fixture.client.currency_limits(&fixture.currency).cumulative_claim_per_cooldown,
        u(&env, 0)
    );
    assert_eq!(project.last_recipient(), None);
}

#[test]
fn signer_validation_precedes_rejecting_smart_account_code() {
    REJECTING_SMART_ACCOUNT_INVOCATIONS.store(0, std::sync::atomic::Ordering::SeqCst);

    fn rejecting_claim_result(
        env: &Env,
        fixture: &ClaimFixture<'_>,
        account: &Address,
        checks: &Vec<ClaimCheck>,
    ) -> Result<Result<(), soroban_sdk::ConversionError>, Result<Error, soroban_sdk::InvokeError>>
    {
        env.set_auths(&smart_account_claim_authorization(
            env,
            account,
            &fixture.client.address,
            &fixture.caller,
            checks,
        ));
        fixture.client.try_claim(&fixture.caller, checks)
    }

    {
        let env = Env::default();
        let fixture = claim_fixture(&env);
        let smart_signer = env.register(RejectingSmartAccount, ());
        let access = FuulAccessControlClient::new(&env, &fixture.client.address);
        access.grant_role(&fixture.client.claim_signer_role(), &smart_signer, &fixture.admin);
        fixture.client.set_required_signers(&fixture.admin, &2);
        let project = register_mock_project(&env, &Address::generate(&env), 0);
        let mut check = claim_check(&env, &fixture, &project.address, 10, 64);
        check.signers = vec![&env, smart_signer.clone()];
        let checks = vec![&env, check];

        assert_eq!(
            rejecting_claim_result(&env, &fixture, &smart_signer, &checks),
            Err(Ok(Error::from_contract_error(6306)))
        );
        assert_eq!(
            REJECTING_SMART_ACCOUNT_INVOCATIONS.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    {
        let env = Env::default();
        let fixture = claim_fixture(&env);
        let smart_signer = env.register(RejectingSmartAccount, ());
        let project = register_mock_project(&env, &Address::generate(&env), 0);
        let mut check = claim_check(&env, &fixture, &project.address, 10, 65);
        check.signers = vec![&env, smart_signer.clone()];
        let checks = vec![&env, check];

        assert_eq!(
            rejecting_claim_result(&env, &fixture, &smart_signer, &checks),
            Err(Ok(Error::from_contract_error(6307)))
        );
        assert_eq!(
            REJECTING_SMART_ACCOUNT_INVOCATIONS.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    {
        let env = Env::default();
        let fixture = claim_fixture(&env);
        let smart_signer = env.register(RejectingSmartAccount, ());
        let access = FuulAccessControlClient::new(&env, &fixture.client.address);
        access.grant_role(&fixture.client.claim_signer_role(), &smart_signer, &fixture.admin);
        let project = register_mock_project(&env, &Address::generate(&env), 0);
        let mut check = claim_check(&env, &fixture, &project.address, 10, 66);
        check.signers = vec![&env, smart_signer.clone(), smart_signer.clone()];
        let checks = vec![&env, check];

        assert_eq!(
            rejecting_claim_result(&env, &fixture, &smart_signer, &checks),
            Err(Ok(Error::from_contract_error(6301)))
        );
        assert_eq!(
            REJECTING_SMART_ACCOUNT_INVOCATIONS.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    {
        let env = Env::default();
        let fixture = claim_fixture(&env);
        let smart_signer = env.register(RejectingSmartAccount, ());
        let access = FuulAccessControlClient::new(&env, &fixture.client.address);
        access.grant_role(&fixture.client.claim_signer_role(), &smart_signer, &fixture.admin);
        let project = register_mock_project(&env, &Address::generate(&env), 0);
        let mut check = claim_check(&env, &fixture, &project.address, 10, 67);
        check.signers = vec![&env, smart_signer.clone()];
        let checks = vec![&env, check];

        let result = rejecting_claim_result(&env, &fixture, &smart_signer, &checks);
        assert_eq!(
            result,
            Err(Ok(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction,)))
        );
        assert_eq!(
            REJECTING_SMART_ACCOUNT_INVOCATIONS.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }
}

#[test]
#[should_panic(expected = "Error(Contract, #6301)")]
fn claim_rejects_duplicate_approved_signers() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    let mut check = claim_check(&env, &fixture, &project.address, 10, 16);
    check.signers = vec![&env, fixture.signer.clone(), fixture.signer.clone()];

    fixture.client.claim(&fixture.caller, &vec![&env, check]);
}

#[test]
fn three_signer_threshold_accepts_three_and_rejects_two() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let second = Address::generate(&env);
    let third = Address::generate(&env);
    let access = FuulAccessControlClient::new(&env, &fixture.client.address);
    let role = fixture.client.claim_signer_role();
    access.grant_role(&role, &second, &fixture.admin);
    access.grant_role(&role, &third, &fixture.admin);
    fixture.client.set_required_signers(&fixture.admin, &3);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    let mut accepted = claim_check(&env, &fixture, &project.address, 10, 32);
    accepted.signers = vec![&env, fixture.signer.clone(), second.clone(), third];

    fixture.client.claim(&fixture.caller, &vec![&env, accepted]);

    let mut rejected = claim_check(&env, &fixture, &project.address, 10, 33);
    rejected.signers = vec![&env, fixture.signer.clone(), second];
    assert_eq!(
        fixture.client.try_claim(&fixture.caller, &vec![&env, rejected]),
        Err(Ok(Error::from_contract_error(6306)))
    );
    assert_eq!(fixture.client.users_claims(&fixture.recipient, &fixture.currency), u(&env, 10));
}
