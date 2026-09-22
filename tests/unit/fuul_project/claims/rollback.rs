use crate::test::*;

#[test]
fn failed_non_fungible_transfer_does_not_consume_the_proof() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 0, 0);
    let currency = env.register(MockNonFungible, ());
    let claim_proof = proof(&env, 14);

    assert!(fixture
        .client
        .try_claim(
            &fixture.manager,
            &fixture.recipient,
            &currency,
            &TokenType::NonFungible,
            &0,
            &u(&env, 7),
            &claim_proof,
            &false,
        )
        .is_err());
    assert!(!fixture.client.claimed_proofs(&claim_proof));
}
