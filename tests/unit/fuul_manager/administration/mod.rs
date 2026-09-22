use crate::test::*;
mod c4;
mod governance;
mod security;
mod source_cases;

#[test]
fn admin_updates_cooldown_and_required_signers_with_exact_events() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_claim_cooldown(&fixture.admin, &172_800);
    assert_eq!(
        env.events().all(),
        std::vec![ClaimCooldownUpdated { period: 172_800 }.to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.claim_cooldown(), 172_800);

    fixture.client.set_required_signers(&fixture.admin, &2);
    assert_eq!(
        env.events().all(),
        std::vec![RequiredSignersUpdated { value: 2 }.to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.required_signers(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn cooldown_rejects_the_current_value() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_claim_cooldown(&fixture.admin, &86_400);
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn cooldown_rejects_values_below_one_day() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_claim_cooldown(&fixture.admin, &86_399);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn cooldown_authentication_happens_before_validation() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.set_claim_cooldown(&fixture.admin, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn required_signers_rejects_zero() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_required_signers(&fixture.admin, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn required_signers_rejects_the_current_value() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_required_signers(&fixture.admin, &1);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn required_signers_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.set_required_signers(&fixture.admin, &2);
}

#[test]
fn role_symbols_are_stable() {
    let env = Env::default();
    let fixture = fixture(&env);

    assert_eq!(fixture.client.pauser_role(), Symbol::new(&env, "pauser"));
    assert_eq!(fixture.client.unpauser_role(), Symbol::new(&env, "unpauser"));
    assert_eq!(fixture.client.claim_signer_role(), Symbol::new(&env, "claim_signer"));
}
