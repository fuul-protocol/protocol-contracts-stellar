use crate::test::*;

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn administrative_setters_require_authorization_before_validation() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.set_default_remove_fee(&fixture.admin, &0);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn default_native_fee_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.set_default_native_claim_fee(&fixture.admin, &30_000);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn project_native_fee_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);
    let project = create_fuul_project(&fixture, &env, "ipfs://auth-native");

    fixture.client.set_native_user_claim_fee(&fixture.admin, &project, &30_000);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn default_project_fee_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.set_default_project_claim_fee(&fixture.admin, &300);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn project_claim_fee_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);
    let project = create_fuul_project(&fixture, &env, "ipfs://auth-claim");

    fixture.client.set_project_claim_fee(&fixture.admin, &project, &300);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn project_remove_fee_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);
    let project = create_fuul_project(&fixture, &env, "ipfs://auth-remove");

    fixture.client.set_remove_fee(&fixture.admin, &project, &300);
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn collector_update_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.set_fee_collector(&fixture.admin, &Address::generate(&env));
}
