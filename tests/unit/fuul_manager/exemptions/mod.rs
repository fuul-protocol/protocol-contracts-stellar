use crate::test::*;
mod security;

#[test]
fn admin_adds_and_removes_fee_exemptions_with_exact_events() {
    let env = Env::default();
    let fixture = fixture(&env);
    let account = Address::generate(&env);
    env.mock_all_auths();

    fixture.client.add_no_claim_fee_address(&fixture.admin, &account);
    assert_eq!(
        env.events().all(),
        std::vec![NoClaimFeeAddressAdded { account: account.clone() }
            .to_xdr(&env, &fixture.client.address)]
    );
    assert!(fixture.client.no_claim_fee_addresses(&account));

    fixture.client.remove_no_claim_fee_address(&fixture.admin, &account);
    assert_eq!(
        env.events().all(),
        std::vec![NoClaimFeeAddressRemoved { account: account.clone() }
            .to_xdr(&env, &fixture.client.address)]
    );
    assert!(!fixture.client.no_claim_fee_addresses(&account));
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn fee_exemption_rejects_a_duplicate_addition() {
    let env = Env::default();
    let fixture = fixture(&env);
    let account = Address::generate(&env);
    env.mock_all_auths();

    fixture.client.add_no_claim_fee_address(&fixture.admin, &account);
    fixture.client.add_no_claim_fee_address(&fixture.admin, &account);
}

#[test]
#[should_panic(expected = "Error(Contract, #6300)")]
fn fee_exemption_rejects_removing_a_missing_address() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.remove_no_claim_fee_address(&fixture.admin, &Address::generate(&env));
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn adding_fee_exemption_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.add_no_claim_fee_address(&fixture.admin, &Address::generate(&env));
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn removing_fee_exemption_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);
    let account = Address::generate(&env);
    env.as_contract(&fixture.client.address, || {
        storage::set_no_claim_fee_address(&env, &account, true);
    });

    fixture.client.remove_no_claim_fee_address(&fixture.admin, &account);
}
