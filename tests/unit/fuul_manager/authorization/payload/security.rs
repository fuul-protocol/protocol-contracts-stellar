use crate::test::{
    roles_pause_helpers as auth,
    security_helpers::{claim_payload, state},
    *,
};

#[test]
fn prepared_claim_approval_is_checked_against_the_current_quorum() {
    let env = auth::test_env();
    let f = claim_fixture(&env);
    let project = register_mock_project(&env, &f.admin, 0);
    let extra = Address::generate(&env);
    FuulAccessControlClient::new(&env, &f.client.address).grant_role(
        &Symbol::new(&env, "claim_signer"),
        &extra,
        &f.admin,
    );
    let checks = vec![&env, claim_check(&env, &f, &project.address, 10, 93)];
    let prepared = claim_payload(&env, &checks.get(0).unwrap());
    f.client.set_required_signers(&f.admin, &2);
    let before = state(&env, &f.client.address);
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
                args: prepared.clone(),
                sub_invokes: &[],
            },
        },
    ]);
    assert_eq!(f.client.try_claim(&f.caller, &checks), Err(Ok(Error::from_contract_error(6306))));
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &f.client.address), before);
    assert_eq!(project.last_recipient(), None);
    auth::authorize(
        &env,
        &f.client.address,
        &f.admin,
        "set_required_signers",
        (&f.admin, 1_u128).into_val(&env),
    );
    f.client.set_required_signers(&f.admin, &1);
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
                args: prepared,
                sub_invokes: &[],
            },
        },
    ]);
    f.client.claim(&f.caller, &checks);
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 10));
}

#[test]
fn independent_wire_payload_rejects_signed_fields_and_invocation_domain() {
    for mutation in 0..8 {
        let env = auth::test_env();
        let f = claim_fixture(&env);
        let project = register_mock_project(&env, &f.admin, 0);
        let check = claim_check(&env, &f, &project.address, 10, 91);
        let checks = vec![&env, check.clone()];
        let mut approved = check;
        let mut contract = f.client.address.clone();
        let mut function = "claim";
        match mutation {
            0 => approved.project_address = Address::generate(&env),
            1 => approved.currency = Address::generate(&env),
            2 => approved.reason = ClaimReason::EndUserPayout,
            3 => approved.token_id = u(&env, 1),
            4 => approved.proof = BytesN::from_array(&env, &[92; 32]),
            5 => approved.to = Address::generate(&env),
            6 => contract = Address::generate(&env),
            _ => function = "other_claim",
        }
        let before = state(&env, &f.client.address);
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
                    contract: &contract,
                    fn_name: function,
                    args: claim_payload(&env, &approved),
                    sub_invokes: &[],
                },
            },
        ]);
        assert_eq!(
            f.client.try_claim(&f.caller, &checks),
            Err(Ok(auth::native_auth_error())),
            "mutation {mutation}"
        );
        auth::assert_auth_failure(&env, 0);
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), before);
        assert_eq!(project.last_recipient(), None);
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
                    args: claim_payload(&env, &checks.get(0).unwrap()),
                    sub_invokes: &[],
                },
            },
        ]);
        f.client.claim(&f.caller, &checks);
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 10));
    }
}
