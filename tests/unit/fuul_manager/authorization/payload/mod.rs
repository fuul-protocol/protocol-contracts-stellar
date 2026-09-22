use crate::test::{
    security_helpers::{authorize_claims, claim_payload},
    *,
};
mod security;

#[test]
fn approved_signer_authorizes_the_exact_business_payload() {
    let env = roles_pause_helpers::test_env();
    let f = claim_fixture(&env);
    let project = register_mock_project(&env, &f.admin, 0);
    let check = claim_check(&env, &f, &project.address, 55, 2);
    let expected = claim_payload(&env, &check);
    f.client.claim(&f.caller, &vec![&env, check]);
    let root = env.auths().into_iter().find(|(a, _)| a == &f.signer).unwrap().1;
    assert_eq!(
        root,
        AuthorizedInvocation {
            function: AuthorizedFunction::Contract((
                f.client.address,
                symbol_short!("claim"),
                expected
            )),
            sub_invocations: std::vec![],
        }
    );
}

#[test]
fn shared_signer_uses_independent_roots_in_input_order() {
    let env = Env::default();
    let f = claim_fixture(&env);
    let project = register_mock_project(&env, &f.admin, 0);
    let checks = vec![
        &env,
        claim_check(&env, &f, &project.address, 10, 47),
        claim_check(&env, &f, &project.address, 20, 48),
    ];
    f.client.claim(&f.caller, &checks);
    let roots: std::vec::Vec<_> =
        env.auths().into_iter().filter(|(a, _)| a == &f.signer).map(|(_, r)| r).collect();
    let expected: std::vec::Vec<_> = checks
        .iter()
        .map(|c| AuthorizedInvocation {
            function: AuthorizedFunction::Contract((
                f.client.address.clone(),
                symbol_short!("claim"),
                claim_payload(&env, &c),
            )),
            sub_invocations: std::vec![],
        })
        .collect();
    assert_eq!(roots, expected);
}

#[test]
fn shared_signer_accepts_reversed_independent_roots() {
    let env = Env::default();
    let f = claim_fixture(&env);
    let project = register_mock_project(&env, &f.admin, 0);
    let first = claim_check(&env, &f, &project.address, 10, 49);
    let second = claim_check(&env, &f, &project.address, 20, 50);
    let checks = vec![&env, first.clone(), second.clone()];
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
                args: claim_payload(&env, &second),
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
    f.client.claim(&f.caller, &checks);
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 30));
    assert_eq!(project.last_recipient(), Some(f.recipient));
}

fn mismatched(field: &str) {
    let env = roles_pause_helpers::test_env();
    let f = claim_fixture(&env);
    let project = register_mock_project(&env, &f.admin, 0);
    let check = claim_check(&env, &f, &project.address, 55, 24);
    let mut altered = check.clone();
    match field {
        "amount" => altered.amount += 1,
        "to" => altered.to = Address::generate(&env),
        "deadline" => altered.deadline = altered.deadline.add(&u(&env, 1)),
        _ => altered.currency_type = TokenType::NonFungible,
    }
    let checks = vec![&env, check.clone()];
    let caller_checks =
        if field == "currency_type" { vec![&env, altered.clone()] } else { checks.clone() };
    let before = security_helpers::state(&env, &f.client.address);
    env.mock_auths(&[
        MockAuth {
            address: &f.caller,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "claim",
                args: (&f.caller, &caller_checks).into_val(&env),
                sub_invokes: &[],
            },
        },
        MockAuth {
            address: &f.signer,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "claim",
                args: claim_payload(&env, &altered),
                sub_invokes: &[],
            },
        },
    ]);
    assert_eq!(
        f.client.try_claim(&f.caller, &checks),
        Err(Ok(roles_pause_helpers::native_auth_error()))
    );
    roles_pause_helpers::assert_auth_failure(&env, 0);
    assert_eq!(security_helpers::state(&env, &f.client.address), before);
    assert!(env.events().all().events().is_empty());
    assert_eq!(project.last_recipient(), None);
    authorize_claims(&env, &f.client.address, &f.caller, &checks, &[]);
    f.client.claim(&f.caller, &checks);
}

#[test]
fn signer_authorization_rejects_a_mismatched_business_payload() {
    mismatched("amount");
}
#[test]
fn signer_authorization_rejects_a_mismatched_recipient() {
    mismatched("to");
}
#[test]
fn signer_authorization_rejects_a_mismatched_deadline() {
    mismatched("deadline");
}
#[test]
fn caller_authorization_rejects_a_mismatched_currency_type() {
    mismatched("currency_type");
}
