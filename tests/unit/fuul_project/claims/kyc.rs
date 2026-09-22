use crate::test::*;

#[test]
#[should_panic(expected = "Error(Contract, #6103)")]
fn claim_enforces_the_project_kyc_requirement() {
    let env = Env::default();
    let fixture = claim_fixture(&env, true, 0, 0);

    fixture.client.claim(
        &fixture.manager,
        &fixture.recipient,
        &fixture.currency,
        &TokenType::StellarAsset,
        &10,
        &u(&env, 0),
        &proof(&env, 5),
        &false,
    );
}

#[test]
fn kyc_registered_recipient_can_claim_from_a_restricted_project() {
    let env = Env::default();
    let fixture = claim_fixture(&env, true, 0, 0);

    fixture.client.claim(
        &fixture.manager,
        &fixture.recipient,
        &fixture.currency,
        &TokenType::StellarAsset,
        &10,
        &u(&env, 0),
        &proof(&env, 6),
        &true,
    );

    assert_eq!(TokenClient::new(&env, &fixture.currency).balance(&fixture.recipient), 10);
    assert_eq!(TokenClient::new(&env, &fixture.currency).balance(&fixture.collector), 0);
    assert_eq!(TokenClient::new(&env, &fixture.currency).balance(&fixture.client.address), 999_990);
    assert!(fixture.client.claimed_proofs(&proof(&env, 6)));
}

#[test]
fn failed_kyc_claim_does_not_consume_the_proof() {
    let env = Env::default();
    let fixture = claim_fixture(&env, true, 0, 0);
    let claim_proof = proof(&env, 11);

    assert!(fixture
        .client
        .try_claim(
            &fixture.manager,
            &fixture.recipient,
            &fixture.currency,
            &TokenType::StellarAsset,
            &10,
            &u(&env, 0),
            &claim_proof,
            &false,
        )
        .is_err());
    assert!(!fixture.client.claimed_proofs(&claim_proof));
}
