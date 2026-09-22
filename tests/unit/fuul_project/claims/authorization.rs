use crate::test::*;

#[test]
fn claim_requires_manager_authorization_for_the_exact_invocation() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 0, 0);
    let claim_proof = proof(&env, 2);

    fixture.client.claim(
        &fixture.manager,
        &fixture.recipient,
        &fixture.currency,
        &TokenType::StellarAsset,
        &10,
        &u(&env, 0),
        &claim_proof,
        &false,
    );

    assert_eq!(
        env.auths(),
        std::vec![(
            fixture.manager.clone(),
            AuthorizedInvocation {
                function: AuthorizedFunction::Contract((
                    fixture.client.address.clone(),
                    symbol_short!("claim"),
                    (
                        fixture.manager,
                        fixture.recipient,
                        fixture.currency,
                        TokenType::StellarAsset,
                        10_i128,
                        u(&env, 0),
                        claim_proof,
                        false,
                    )
                        .into_val(&env),
                )),
                sub_invocations: std::vec![],
            },
        )]
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6101)")]
fn claim_rejects_an_account_without_the_factory_manager_role() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 0, 0);
    let other = Address::generate(&env);

    fixture.client.claim(
        &other,
        &fixture.recipient,
        &fixture.currency,
        &TokenType::StellarAsset,
        &10,
        &u(&env, 0),
        &proof(&env, 3),
        &false,
    );
}
