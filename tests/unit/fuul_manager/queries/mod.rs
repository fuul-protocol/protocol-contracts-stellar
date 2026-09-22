use crate::test::*;
mod ttl;

#[test]
fn unknown_currency_and_claim_getters_match_evm_mapping_defaults() {
    let env = Env::default();
    let fixture = fixture(&env);
    let unknown = Address::generate(&env);
    let user = Address::generate(&env);

    assert_eq!(fixture.client.currency_limits(&unknown), CurrencyTokenLimit::zero(&env));
    assert_eq!(fixture.client.users_claims(&user, &unknown), u(&env, 0));
    assert!(!fixture.client.no_claim_fee_addresses(&user));
}
