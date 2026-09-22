use crate::test::*;

#[test]
fn claim_transfers_a_sep50_non_fungible_token() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 500, 20_000);
    let currency = env.register(MockNonFungible, ());
    let token = MockNonFungibleClient::new(&env, &currency);
    token.mint(&fixture.client.address, &7);
    let claim_proof = proof(&env, 7);

    let result = fixture.client.claim(
        &fixture.manager,
        &fixture.recipient,
        &currency,
        &TokenType::NonFungible,
        &0,
        &u(&env, 7),
        &claim_proof,
        &false,
    );

    assert_eq!(token.owner_of(&7), fixture.recipient);
    assert!(fixture.client.claimed_proofs(&claim_proof));
    assert_eq!(
        result,
        ProjectClaimResult { native_user_claim_fee: 20_000, fee_collector: fixture.collector }
    );
}

#[test]
fn claim_transfers_one_unit_from_a_multi_token_collection() {
    let env = Env::default();
    let fixture = claim_fixture(&env, false, 500, 20_000);
    let currency = env.register(MockMultiToken, ());
    let token = MockMultiTokenClient::new(&env, &currency);
    token.mint(&fixture.client.address, &9, &10);

    fixture.client.claim(
        &fixture.manager,
        &fixture.recipient,
        &currency,
        &TokenType::MultiToken,
        &0,
        &u(&env, 9),
        &proof(&env, 13),
        &false,
    );

    assert_eq!(token.balance(&fixture.client.address, &9), 9);
    assert_eq!(token.balance(&fixture.recipient, &9), 1);
    assert_eq!(token.balance(&fixture.collector, &9), 0);
}
