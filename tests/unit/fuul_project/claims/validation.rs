use crate::test::*;

#[test]
fn claim_rejects_a_non_fungible_id_above_the_sep50_range() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 0, 0);
    let currency = env.register(MockNonFungible, ());
    let claim_proof = proof(&env, 15);
    let token_id = u(&env, u128::from(u32::MAX) + 1);

    assert!(fixture
        .client
        .try_claim(
            &fixture.manager,
            &fixture.recipient,
            &currency,
            &TokenType::NonFungible,
            &0,
            &token_id,
            &claim_proof,
            &false,
        )
        .is_err());
    assert!(!fixture.client.claimed_proofs(&claim_proof));
}

#[test]
#[should_panic(expected = "Error(Contract, #6105)")]
fn claim_rejects_negative_soroban_amounts() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 0, 0);

    fixture.client.claim(
        &fixture.manager,
        &fixture.recipient,
        &fixture.currency,
        &TokenType::StellarAsset,
        &-1,
        &u(&env, 0),
        &proof(&env, 8),
        &false,
    );
}

#[test]
fn configured_fee_accepts_an_amount_that_rounds_the_fee_to_zero() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 100, 0);

    fixture.client.claim(
        &fixture.manager,
        &fixture.recipient,
        &fixture.currency,
        &TokenType::StellarAsset,
        &99,
        &u(&env, 0),
        &proof(&env, 9),
        &false,
    );
    assert_eq!(TokenClient::new(&env, &fixture.currency).balance(&fixture.recipient), 99);
    assert_eq!(TokenClient::new(&env, &fixture.currency).balance(&fixture.collector), 0);
}
