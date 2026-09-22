use crate::test::*;

#[test]
#[should_panic(expected = "Error(Contract, #6102)")]
fn claim_rejects_a_reused_proof() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 0, 0);
    let claim_proof = proof(&env, 4);

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
}

#[test]
fn archived_claim_proof_restores_and_still_rejects_replay() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 0, 0);
    let claim_proof = proof(&env, 40);

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

    env.ledger().set_sequence_number(env.ledger().sequence() + INSTANCE_EXTEND_AMOUNT + 1);

    assert!(fixture.client.claimed_proofs(&claim_proof));
    let restoration = env.cost_estimate().resources();
    assert!(restoration.write_entries >= 2);
    let token = TokenClient::new(&env, &fixture.currency);
    let balances = [
        token.balance(&fixture.client.address),
        token.balance(&fixture.recipient),
        token.balance(&fixture.collector),
    ];
    assert_eq!(
        fixture.client.try_claim(
            &fixture.manager,
            &fixture.recipient,
            &fixture.currency,
            &TokenType::StellarAsset,
            &10,
            &u(&env, 0),
            &claim_proof,
            &false,
        ),
        Err(Ok(soroban_sdk::Error::from_contract_error(6102)))
    );
    assert_eq!(
        [
            token.balance(&fixture.client.address),
            token.balance(&fixture.recipient),
            token.balance(&fixture.collector)
        ],
        balances
    );
    assert!(fixture.client.claimed_proofs(&claim_proof));
}
