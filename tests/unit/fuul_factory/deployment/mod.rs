use crate::test::*;

mod boundaries;

#[test]
fn project_creation_deploys_and_initializes_the_project() {
    crate::test::integration::creation::historical_project_initialization();
}

#[test]
fn deployed_project_reads_updated_factory_fee_during_removal() {
    crate::test::integration::creation::historical_project_removal();
}

#[test]
fn project_creation_uses_unique_deterministic_tracker_salts() {
    let env = Env::default();
    let fixture = fixture(&env);

    let first = create_fuul_project(&fixture, &env, "ipfs://one");
    let second = create_fuul_project(&fixture, &env, "ipfs://two");

    assert_ne!(first, second);
    assert_eq!(fixture.client.contract_tracker(), 2);
}

#[test]
fn project_tracker_overflow_is_rejected_before_deployment() {
    let env = Env::default();
    let fixture = fixture(&env);
    // Values above uint96 cannot result from normal counter updates.
    creation_helpers::seed_tracker(&env, &fixture.client.address, 1_u128 << 96);
    let before = authority::state(&env, &fixture.client.address);

    assert!(fixture
        .client
        .try_create_fuul_project(
            &fixture.project_admin,
            &String::from_str(&env, "ipfs://overflow"),
            &false,
        )
        .is_err());
    assert_eq!(authority::state(&env, &fixture.client.address), before);
}

#[test]
#[should_panic(expected = "Error(Contract, #6200)")]
fn project_creation_rejects_an_empty_uri() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.create_fuul_project(&fixture.project_admin, &String::from_str(&env, ""), &false);
}

#[test]
fn project_creation_emits_the_exact_factory_event() {
    let env = Env::default();
    let fixture = fixture(&env);
    let uri = String::from_str(&env, "ipfs://event-project");

    let project = fixture.client.create_fuul_project(&fixture.project_admin, &uri, &false);

    assert_eq!(
        env.events().all().filter_by_contract(&fixture.client.address),
        std::vec![ProjectCreated {
            project_id: 1,
            deployed_address: project,
            project_info_uri: uri
        }
        .to_xdr(&env, &fixture.client.address)]
    );
}
