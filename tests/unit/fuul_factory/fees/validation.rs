use crate::test::*;

#[test]
fn basis_point_fees_accept_one_hundred_percent() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let project = create_fuul_project(&fixture, &env, "ipfs://maximum");

    fixture.client.set_default_project_claim_fee(&fixture.admin, &10_000);
    fixture.client.set_project_claim_fee(&fixture.admin, &project, &10_000);
    fixture.client.set_default_remove_fee(&fixture.admin, &10_000);
    fixture.client.set_remove_fee(&fixture.admin, &project, &10_000);

    assert_eq!(fixture.client.default_project_claim_fee(), 10_000);
    assert_eq!(fixture.client.default_remove_fee(), 10_000);
    assert_eq!(fixture.client.project_fees(&project).project_claim_fee, 10_000);
    assert_eq!(fixture.client.project_fees(&project).remove_fee, 10_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn basis_point_fees_reject_values_above_one_hundred_percent() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_default_project_claim_fee(&fixture.admin, &10_001);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn setters_reject_unchanged_values() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_default_remove_fee(&fixture.admin, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn default_native_fee_rejects_its_current_value() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_default_native_claim_fee(&fixture.admin, &20_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn project_native_fee_rejects_its_current_value() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let project = create_fuul_project(&fixture, &env, "ipfs://same-native");

    fixture.client.set_native_user_claim_fee(&fixture.admin, &project, &20_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn default_project_fee_rejects_its_current_value() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_default_project_claim_fee(&fixture.admin, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn project_claim_fee_rejects_its_current_value() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let project = create_fuul_project(&fixture, &env, "ipfs://same-claim");

    fixture.client.set_project_claim_fee(&fixture.admin, &project, &100);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn project_remove_fee_rejects_its_current_value() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let project = create_fuul_project(&fixture, &env, "ipfs://same-remove");

    fixture.client.set_remove_fee(&fixture.admin, &project, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn project_claim_fee_rejects_more_than_one_hundred_percent() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let project = create_fuul_project(&fixture, &env, "ipfs://invalid-claim");

    fixture.client.set_project_claim_fee(&fixture.admin, &project, &10_001);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn default_remove_fee_rejects_more_than_one_hundred_percent() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_default_remove_fee(&fixture.admin, &10_001);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn project_remove_fee_rejects_more_than_one_hundred_percent() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let project = create_fuul_project(&fixture, &env, "ipfs://invalid-remove");

    fixture.client.set_remove_fee(&fixture.admin, &project, &10_001);
}

#[test]
fn project_remove_fee_can_return_to_zero() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let project = create_fuul_project(&fixture, &env, "ipfs://zero-remove");

    fixture.client.set_remove_fee(&fixture.admin, &project, &500);
    fixture.client.set_remove_fee(&fixture.admin, &project, &0);

    assert_eq!(fixture.client.project_fees(&project).remove_fee, 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn native_fee_setters_reject_negative_values() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_default_native_claim_fee(&fixture.admin, &-1);
}

#[test]
#[should_panic(expected = "Error(Contract, #6201)")]
fn collector_setter_rejects_the_current_collector() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_fee_collector(&fixture.admin, &fixture.collector);
}
