use crate::test::*;

mod economics;
mod initialization;
mod roles;
mod rollback;
mod ttl;

#[test]
fn constructor_sets_configuration_roles_and_initial_limits() {
    let env = Env::default();
    let fixture = fixture(&env);
    let access = FuulAccessControlClient::new(&env, &fixture.client.address);

    assert_eq!(
        access.get_role_members(&access.default_admin_role()),
        vec![&env, fixture.admin.clone()]
    );
    assert_role(&env, &fixture, &fixture.pauser, &fixture.client.pauser_role());
    assert_role(&env, &fixture, &fixture.unpauser, &fixture.client.unpauser_role());
    assert_role(&env, &fixture, &fixture.signer, &fixture.client.claim_signer_role());
    assert_eq!(fixture.client.claim_cooldown(), 86_400);
    assert_eq!(fixture.client.min_claim_cooldown(), 86_400);
    assert_eq!(fixture.client.required_signers(), 1);
    assert_eq!(fixture.client.kyc_validator(), Some(fixture.validator));
    assert_eq!(fixture.client.native_asset(), fixture.native_asset);
    assert!(!fixture.client.paused());

    let expected = CurrencyTokenLimit {
        claim_limit_per_cooldown: u(&env, 1_000_000_000_000_i128),
        cumulative_claim_per_cooldown: u(&env, 0),
        claim_cooldown_period_started: 1_000_000,
    };
    assert_eq!(fixture.client.currency_limits(&fixture.accepted_currency), expected);
    assert_eq!(fixture.client.currency_limits(&fixture.native_asset), expected);
}

#[test]
fn constructor_grants_all_unique_initial_signers() {
    let env = Env::default();
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    let third = Address::generate(&env);
    let accepted = Address::generate(&env);
    let native = Address::generate(&env);
    let (client, _, _, _) = register_manager(
        &env,
        3,
        vec![&env, first.clone(), second.clone(), third.clone()],
        &accepted,
        &native,
        None,
    );
    let access = FuulAccessControlClient::new(&env, &client.address);
    let role = client.claim_signer_role();

    assert!(access.has_role(&role, &first));
    assert!(access.has_role(&role, &second));
    assert!(access.has_role(&role, &third));
    assert_eq!(client.required_signers(), 3);
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn constructor_rejects_zero_required_signers() {
    let env = Env::default();
    let signer = Address::generate(&env);
    let accepted = Address::generate(&env);
    let native = Address::generate(&env);

    register_manager(&env, 0, vec![&env, signer], &accepted, &native, None);
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn constructor_rejects_an_empty_signer_list() {
    let env = Env::default();
    let accepted = Address::generate(&env);
    let native = Address::generate(&env);

    register_manager(&env, 1, Vec::new(&env), &accepted, &native, None);
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn constructor_rejects_a_threshold_above_the_signer_count() {
    let env = Env::default();
    let signer = Address::generate(&env);
    let accepted = Address::generate(&env);
    let native = Address::generate(&env);

    register_manager(&env, 2, vec![&env, signer], &accepted, &native, None);
}

#[test]
#[should_panic(expected = "Error(Contract, #6301)")]
fn constructor_rejects_duplicate_signers() {
    let env = Env::default();
    let signer = Address::generate(&env);
    let accepted = Address::generate(&env);
    let native = Address::generate(&env);

    register_manager(&env, 2, vec![&env, signer.clone(), signer], &accepted, &native, None);
}

#[test]
#[should_panic(expected = "Error(Contract, #6302)")]
fn constructor_rejects_the_same_initial_currency_twice() {
    let env = Env::default();
    let signer = Address::generate(&env);
    let currency = Address::generate(&env);

    register_manager(&env, 1, vec![&env, signer], &currency, &currency, None);
}
