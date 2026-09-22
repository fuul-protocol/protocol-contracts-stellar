use crate::test::*;
use stellar_access::access_control as oz;

#[test]
fn constructor_does_not_seed_legacy_singleton_authority() {
    let env = authority::test_env();
    let f = fixture(&env);
    env.as_contract(&f.client.address, || assert_eq!(oz::get_admin(&env), None));
}
