use crate::test::{
    roles_pause_helpers as auth,
    security_helpers::{authorize_claims, claim_payload, state},
    *,
};

#[test]
fn empty_claim_requires_explicit_caller_auth_and_is_a_protocol_noop() {
    let env = auth::test_env();
    let f = claim_fixture(&env);
    let validator = env.register(PanickingKyc, ());
    f.client.set_kyc_validator(&f.admin, &Some(validator));
    let checks = Vec::<ClaimCheck>::new(&env);
    let before = state(&env, &f.client.address);
    env.set_auths(&[]);
    assert_eq!(f.client.try_claim(&f.caller, &checks), Err(Ok(auth::native_auth_error())));
    assert_eq!(state(&env, &f.client.address), before);
    assert!(env.events().all().events().is_empty());
    authorize_claims(&env, &f.client.address, &f.caller, &checks, &[]);
    f.client.claim(&f.caller, &checks);
    assert_eq!(
        env.auths(),
        std::vec![(
            f.caller.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    f.client.address.clone(),
                    symbol_short!("claim"),
                    (&f.caller, &checks).into_val(&env)
                )),
                sub_invocations: std::vec![],
            }
        )]
    );
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &f.client.address), before);
}

#[test]
fn signer_can_also_be_the_fee_payer_with_independent_auth_roots() {
    let env = Env::default();
    let f = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 20_000);
    StellarAssetClient::new(&env, &f.native_asset).mint(&f.signer, &20_000);
    f.client.claim(&f.signer, &vec![&env, claim_check(&env, &f, &project.address, 10, 22)]);
    assert_eq!(env.auths().iter().filter(|(a, _)| a == &f.signer).count(), 2);
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 10));
    let native = TokenClient::new(&env, &f.native_asset);
    assert_eq!(native.balance(&f.signer), 0);
    assert_eq!(native.balance(&collector), 20_000);
}

#[test]
fn caller_signer_on_only_check_two_uses_distinct_consent_and_proof_roots() {
    let env = Env::default();
    let f = claim_fixture(&env);
    FuulAccessControlClient::new(&env, &f.client.address).grant_role(
        &f.client.claim_signer_role(),
        &f.caller,
        &f.admin,
    );
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 20_000);
    let first = claim_check(&env, &f, &project.address, 10, 59);
    let mut second = claim_check(&env, &f, &project.address, 20, 60);
    second.signers = vec![&env, f.caller.clone()];
    let checks = vec![&env, first.clone(), second.clone()];
    f.client.claim(&f.caller, &checks);
    let roots = env.auths();
    let caller_roots: std::vec::Vec<_> =
        roots.iter().filter(|(a, _)| a == &f.caller).map(|(_, r)| r.clone()).collect();
    assert_eq!(
        caller_roots,
        std::vec![
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    f.client.address.clone(),
                    symbol_short!("claim"),
                    (&f.caller, &checks).into_val(&env)
                )),
                sub_invocations: std::vec![AuthorizedInvocation {
                    function: AuthorizedFunction::Contract((
                        f.native_asset.clone(),
                        symbol_short!("transfer"),
                        (&f.caller, MuxedAddress::from(&collector), 40_000_i128).into_val(&env)
                    )),
                    sub_invocations: std::vec![],
                }],
            },
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    f.client.address.clone(),
                    symbol_short!("claim"),
                    claim_payload(&env, &second)
                )),
                sub_invocations: std::vec![],
            },
        ]
    );
    assert_eq!(
        roots
            .iter()
            .filter(|(a, _)| a == &f.signer)
            .map(|(_, r)| r.clone())
            .collect::<std::vec::Vec<_>>(),
        std::vec![AuthorizedInvocation {
            function: AuthorizedFunction::Contract((
                f.client.address.clone(),
                symbol_short!("claim"),
                claim_payload(&env, &first)
            )),
            sub_invocations: std::vec![],
        }]
    );
    assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&collector), 40_000);
}

#[test]
fn caller_consent_precedes_checks_but_later_signer_auth_follows_earlier_project() {
    let env = Env::default();
    let f = claim_fixture(&env);
    FuulAccessControlClient::new(&env, &f.client.address).grant_role(
        &f.client.claim_signer_role(),
        &f.caller,
        &f.admin,
    );
    let failing = env.register(ReplayRejectingProject, ());
    let later = register_mock_project(&env, &f.admin, 0);
    let first = claim_check(&env, &f, &failing, 10, 61);
    let mut second = claim_check(&env, &f, &later.address, 20, 62);
    second.signers = vec![&env, f.caller.clone()];
    let checks = vec![&env, first.clone(), second];
    env.mock_auths(&[
        MockAuth {
            address: &f.caller,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "claim",
                args: (&f.caller, &checks).into_val(&env),
                sub_invokes: &[],
            },
        },
        MockAuth {
            address: &f.signer,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "claim",
                args: claim_payload(&env, &first),
                sub_invokes: &[],
            },
        },
    ]);
    assert_eq!(f.client.try_claim(&f.caller, &checks), Err(Ok(Error::from_contract_error(6102))));
    assert_eq!(later.last_recipient(), None);
    assert!(env.events().all().events().is_empty());
}

#[test]
fn fee_exempt_signer_caller_cannot_authorize_only_part_of_a_batch() {
    let env = auth::test_env();
    let f = claim_fixture(&env);
    FuulAccessControlClient::new(&env, &f.client.address).grant_role(
        &f.client.claim_signer_role(),
        &f.caller,
        &f.admin,
    );
    f.client.add_no_claim_fee_address(&f.admin, &f.caller);
    let project = register_mock_project(&env, &f.admin, 20_000);
    let mut first = claim_check(&env, &f, &project.address, 10, 43);
    first.signers = vec![&env, f.caller.clone()];
    let second = claim_check(&env, &f, &project.address, 20, 44);
    let checks = vec![&env, first.clone(), second.clone()];
    let before = state(&env, &f.client.address);
    env.mock_auths(&[
        MockAuth {
            address: &f.caller,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "claim",
                args: claim_payload(&env, &first),
                sub_invokes: &[],
            },
        },
        MockAuth {
            address: &f.signer,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "claim",
                args: claim_payload(&env, &second),
                sub_invokes: &[],
            },
        },
    ]);
    assert_eq!(f.client.try_claim(&f.caller, &checks), Err(Ok(auth::native_auth_error())));
    auth::assert_auth_failure(&env, 0);
    assert_eq!(state(&env, &f.client.address), before);
    assert!(env.events().all().events().is_empty());
    assert_eq!(project.last_recipient(), None);
}

#[test]
fn fee_exempt_signer_caller_authorizes_full_invocation_and_own_proof() {
    let env = Env::default();
    let f = claim_fixture(&env);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 20_000);
    FuulAccessControlClient::new(&env, &f.client.address).grant_role(
        &f.client.claim_signer_role(),
        &f.caller,
        &f.admin,
    );
    f.client.add_no_claim_fee_address(&f.admin, &f.caller);
    let mut first = claim_check(&env, &f, &project.address, 10, 45);
    first.signers = vec![&env, f.caller.clone()];
    let checks = vec![&env, first, claim_check(&env, &f, &project.address, 20, 46)];
    authorize_claims(&env, &f.client.address, &f.caller, &checks, &[]);
    f.client.claim(&f.caller, &checks);
    assert_eq!(env.auths().iter().filter(|(a, _)| a == &f.caller).count(), 2);
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 30));
    assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&f.caller), 1_000_000);
    assert_eq!(TokenClient::new(&env, &f.native_asset).balance(&collector), 0);
}
