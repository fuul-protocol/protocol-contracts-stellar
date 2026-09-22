use crate::test::*;

#[test]
fn unknown_project_addresses_match_evm_mapping_defaults() {
    let env = Env::default();
    let fixture = fixture(&env);
    let unknown = Address::generate(&env);

    assert_eq!(
        fixture.client.project_fees(&unknown),
        ProjectFees { native_user_claim_fee: 0, project_claim_fee: 0, remove_fee: 0 }
    );
    let information = fixture.client.get_fees_information(&unknown);
    assert_eq!(information.fee_collector, fixture.collector);
    assert_eq!(information.fees, fixture.client.project_fees(&unknown));
}

#[test]
fn projects_snapshot_defaults_at_creation_time() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_default_native_claim_fee(&fixture.admin, &30_000);
    fixture.client.set_default_project_claim_fee(&fixture.admin, &300);
    fixture.client.set_default_remove_fee(&fixture.admin, &250);
    let first = create_fuul_project(&fixture, &env, "ipfs://first-defaults");

    fixture.client.set_default_native_claim_fee(&fixture.admin, &40_000);
    fixture.client.set_default_project_claim_fee(&fixture.admin, &500);
    fixture.client.set_default_remove_fee(&fixture.admin, &750);
    let second = create_fuul_project(&fixture, &env, "ipfs://second-defaults");

    assert_eq!(
        fixture.client.project_fees(&first),
        ProjectFees { native_user_claim_fee: 30_000, project_claim_fee: 300, remove_fee: 250 }
    );
    assert_eq!(
        fixture.client.project_fees(&second),
        ProjectFees { native_user_claim_fee: 40_000, project_claim_fee: 500, remove_fee: 750 }
    );
}

#[test]
fn get_fees_returns_the_collector_and_complete_project_fees() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let project = create_fuul_project(&fixture, &env, "ipfs://complete-fees");
    let collector = Address::generate(&env);

    fixture.client.set_fee_collector(&fixture.admin, &collector);
    fixture.client.set_native_user_claim_fee(&fixture.admin, &project, &45_000);
    fixture.client.set_project_claim_fee(&fixture.admin, &project, &450);
    fixture.client.set_remove_fee(&fixture.admin, &project, &650);

    assert_eq!(
        fixture.client.get_fees_information(&project),
        FeesInformation {
            fee_collector: collector,
            fees: ProjectFees {
                native_user_claim_fee: 45_000,
                project_claim_fee: 450,
                remove_fee: 650,
            },
        }
    );
}

#[test]
fn per_project_fee_updates_do_not_change_other_projects() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let first = create_fuul_project(&fixture, &env, "ipfs://first");
    let second = create_fuul_project(&fixture, &env, "ipfs://second");

    fixture.client.set_native_user_claim_fee(&fixture.admin, &first, &50_000);
    fixture.client.set_project_claim_fee(&fixture.admin, &first, &1_000);
    fixture.client.set_remove_fee(&fixture.admin, &first, &500);

    assert_eq!(
        fixture.client.project_fees(&second),
        ProjectFees { native_user_claim_fee: 20_000, project_claim_fee: 100, remove_fee: 0 }
    );
}
