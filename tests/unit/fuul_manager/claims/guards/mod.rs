use crate::test::*;

#[test]
fn paused_manager_rejects_an_empty_batch_without_changes() {
    for compiled in [false, true] {
        let e = Env::default();
        let input = crate::test::constructor_helpers::Bootstrap::new(&e);
        let id = if compiled {
            e.register(crate::test::constructor_helpers::MANAGER_WASM, input.args(&e))
        } else {
            input.register(&e)
        };
        let c = FuulManagerClient::new(&e, &id);
        e.mock_all_auths();
        c.pause(&input.pauser);
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            c.try_claim(&input.admin, &Vec::new(&e)),
            Err(Ok(Error::from_contract_error(1000)))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
    }
}

#[test]
#[should_panic(expected = "Error(Contract, #1000)")]
fn paused_manager_rejects_claims() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    fixture.client.pause(&fixture.pauser);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);

    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 10, 20)]);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn claim_requires_the_explicit_caller_authorization() {
    let env = Env::default();
    let fixture = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 0);
    env.set_auths(&[]);

    fixture
        .client
        .claim(&fixture.caller, &vec![&env, claim_check(&env, &fixture, &project.address, 10, 21)]);
}
