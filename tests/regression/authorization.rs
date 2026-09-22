use super::*;
use soroban_sdk::testutils::{AuthorizedFunction, AuthorizedInvocation};

// Literal fields are an independent oracle for the signed wire format.
fn payload(e: &Env, c: &ClaimCheck) -> Vec<Val> {
    (Map::<Symbol, Val>::from_array(
        e,
        [
            (Symbol::new(e, "project_address"), c.project_address.clone().into_val(e)),
            (Symbol::new(e, "to"), c.to.clone().into_val(e)),
            (Symbol::new(e, "currency"), c.currency.clone().into_val(e)),
            (Symbol::new(e, "amount"), c.amount.into_val(e)),
            (
                Symbol::new(e, "reason"),
                (Symbol::new(
                    e,
                    match c.reason {
                        ClaimReason::AffiliatePayout => "AffiliatePayout",
                        ClaimReason::EndUserPayout => "EndUserPayout",
                    },
                ),)
                    .into_val(e),
            ),
            (Symbol::new(e, "token_id"), c.token_id.clone().into_val(e)),
            (Symbol::new(e, "deadline"), c.deadline.clone().into_val(e)),
            (Symbol::new(e, "proof"), c.proof.clone().into_val(e)),
        ],
    ),)
        .into_val(e)
}

fn authorize(f: &Fixture<'_>, checks: &Vec<ClaimCheck>, signed: &Vec<ClaimCheck>, caller: bool) {
    let caller_root = MockAuthInvoke {
        contract: &f.manager.address,
        fn_name: "claim",
        args: (&f.caller, checks).into_val(f.e),
        sub_invokes: &[],
    };
    let roots: std::vec::Vec<_> = signed
        .iter()
        .flat_map(|c| {
            c.signers
                .iter()
                .map(|a| {
                    (
                        a,
                        MockAuthInvoke {
                            contract: &f.manager.address,
                            fn_name: "claim",
                            args: payload(f.e, &c),
                            sub_invokes: &[],
                        },
                    )
                })
                .collect::<std::vec::Vec<_>>()
        })
        .collect();
    let mut auth = std::vec::Vec::new();
    if caller {
        auth.push(MockAuth { address: &f.caller, invoke: &caller_root });
    }
    auth.extend(roots.iter().map(|(a, r)| MockAuth { address: a, invoke: r }));
    f.e.mock_auths(&auth);
}

#[test]
fn each_claim_requires_its_own_exact_signer_authorization_root() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let checks = vec![&e, f.check(30, 100), f.check(31, 200)];
    f.manager.claim(&f.caller, &checks);
    let roots: std::vec::Vec<_> =
        e.auths().into_iter().filter(|(a, _)| a == &f.signer).map(|(_, r)| r).collect();
    let expected: std::vec::Vec<_> = checks
        .iter()
        .map(|c| AuthorizedInvocation {
            function: AuthorizedFunction::Contract((
                f.manager.address.clone(),
                symbol_short!("claim"),
                payload(&e, &c),
            )),
            sub_invocations: std::vec![],
        })
        .collect();
    assert_eq!(roots, expected);
}

#[test]
fn every_signed_field_is_bound_and_caller_consent_is_required() {
    for field in 0..9 {
        let e = Env::default();
        let f = Fixture::new(&e);
        let c = f.check(32, 100);
        f.factory.set_native_user_claim_fee(&f.factory_admin, &f.project.address, &0);
        let mut altered = c.clone();
        match field {
            0 => altered.project_address = Address::generate(&e),
            1 => altered.to = Address::generate(&e),
            2 => altered.currency = Address::generate(&e),
            3 => altered.amount += 1,
            4 => altered.reason = ClaimReason::EndUserPayout,
            5 => altered.token_id = u(&e, 1),
            6 => altered.deadline = u(&e, 1_000_301),
            7 => altered.proof = BytesN::from_array(&e, &[99; 32]),
            _ => {}
        }
        let checks = vec![&e, c.clone()];
        authorize(&f, &checks, &vec![&e, altered], field != 8);
        assert!(f.manager.try_claim(&f.caller, &checks).is_err(), "field {field}");
        assert!(e.events().all().events().is_empty());
        f.assert_unsettled(&c);
        authorize(&f, &checks, &checks, true);
        f.claim(&c);
        assert_eq!(f.balances(), [999_899, 100, 1, 1_000_000, 0]);
    }
}

#[test]
fn shared_signer_and_caller_overlap_still_need_all_independent_roots() {
    let e = Env::default();
    let mut f = Fixture::new(&e);
    f.caller = f.signer.clone();
    f.factory.set_native_user_claim_fee(&f.factory_admin, &f.project.address, &0);
    let checks = vec![&e, f.check(33, 100), f.check(34, 200)];
    authorize(&f, &checks, &vec![&e, checks.get(0).unwrap()], true);
    assert!(f.manager.try_claim(&f.caller, &checks).is_err());
    assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.recipient), 0);
    assert!(!f.project.claimed_proofs(&checks.get(0).unwrap().proof));
    authorize(&f, &checks, &vec![&e, checks.get(1).unwrap(), checks.get(0).unwrap()], true);
    f.manager.claim(&f.caller, &checks);
    assert_eq!(f.manager.users_claims(&f.recipient, &f.currency), u(&e, 300));
}

#[test]
fn two_signer_quorum_needs_two_distinct_current_role_members() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let other = Address::generate(&e);
    let role = f.manager.claim_signer_role();
    let access = FuulAccessControlClient::new(&e, &f.manager.address);
    access.grant_role(&role, &other, &f.admin);
    f.manager.set_required_signers(&f.admin, &2);
    let mut c = f.check(35, 100);
    assert_eq!(f.manager.try_claim(&f.caller, &vec![&e, c.clone()]), Err(Ok(error(6306))));
    c.signers.push_back(other.clone());
    access.revoke_role(&role, &other, &f.admin);
    assert_eq!(f.manager.try_claim(&f.caller, &vec![&e, c.clone()]), Err(Ok(error(6307))));
    access.grant_role(&role, &other, &f.admin);
    f.claim(&c);
}

#[test]
fn project_rejects_claims_from_an_unregistered_manager() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let c = f.check(36, 100);
    assert_eq!(
        f.project.try_claim(
            &f.caller,
            &f.recipient,
            &f.currency,
            &c.currency_type,
            &c.amount,
            &c.token_id,
            &c.proof,
            &true
        ),
        Err(Ok(error(6101)))
    );
    f.assert_unsettled(&c);
}
