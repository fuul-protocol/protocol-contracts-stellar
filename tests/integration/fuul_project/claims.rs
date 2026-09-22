use crate::test::*;

#[test]
fn manager_claim_transfers_the_amount_and_fee_and_marks_the_proof() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 100, 20_000);
    let claim_proof = proof(&env, 1);
    let token = TokenClient::new(&env, &fixture.currency);

    let result = fixture.client.claim(
        &fixture.manager,
        &fixture.recipient,
        &fixture.currency,
        &TokenType::StellarAsset,
        &100_000,
        &u(&env, 0),
        &claim_proof,
        &false,
    );

    assert_eq!(token.balance(&fixture.recipient), 100_000);
    assert_eq!(token.balance(&fixture.collector), 1_000);
    assert_eq!(token.balance(&fixture.client.address), 899_000);
    assert!(fixture.client.claimed_proofs(&claim_proof));
    assert_eq!(
        result,
        ProjectClaimResult { native_user_claim_fee: 20_000, fee_collector: fixture.collector }
    );
}

#[test]
fn failed_token_transfer_does_not_consume_the_proof() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 0, 0);
    let claim_proof = proof(&env, 12);

    assert!(fixture
        .client
        .try_claim(
            &fixture.manager,
            &fixture.recipient,
            &fixture.currency,
            &TokenType::StellarAsset,
            &1_000_001,
            &u(&env, 0),
            &claim_proof,
            &false,
        )
        .is_err());
    assert!(!fixture.client.claimed_proofs(&claim_proof));
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn maximum_claim_rejects_insufficient_token_balance() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 10_000, 0);

    fixture.client.claim(
        &fixture.manager,
        &fixture.recipient,
        &fixture.currency,
        &TokenType::StellarAsset,
        &i128::MAX,
        &u(&env, 0),
        &proof(&env, 10),
        &false,
    );
}

#[test]
fn test_fixture_uses_a_live_stellar_asset_contract() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 0, 0);

    assert_eq!(StellarAssetClient::new(&env, &fixture.currency).admin(), fixture.token_admin);
}
