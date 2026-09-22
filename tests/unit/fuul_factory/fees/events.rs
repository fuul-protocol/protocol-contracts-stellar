use crate::test::*;

#[test]
fn fee_setters_update_values_and_publish_exact_events() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let project = create_fuul_project(&fixture, &env, "ipfs://fees");

    let collector = Address::generate(&env);
    fixture.client.set_fee_collector(&fixture.admin, &collector);
    assert_eq!(
        env.events().all(),
        std::vec![FeeCollectorUpdated { new_collector: collector.clone() }
            .to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.fee_collector(), collector);

    fixture.client.set_default_native_claim_fee(&fixture.admin, &35_000);
    assert_eq!(
        env.events().all(),
        std::vec![DefaultNativeClaimFeeUpdated { new_default_native_claim_fee: 35_000 }
            .to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.default_native_claim_fee(), 35_000);

    fixture.client.set_native_user_claim_fee(&fixture.admin, &project, &45_000);
    assert_eq!(
        env.events().all(),
        std::vec![NativeClaimFeeUpdated {
            project_address: project.clone(),
            native_claim_fee: 45_000
        }
        .to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.project_fees(&project).native_user_claim_fee, 45_000);

    fixture.client.set_default_project_claim_fee(&fixture.admin, &350);
    assert_eq!(
        env.events().all(),
        std::vec![DefaultProjectClaimFeeUpdated { new_project_claim_fee: 350 }
            .to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.default_project_claim_fee(), 350);

    fixture.client.set_project_claim_fee(&fixture.admin, &project, &450);
    assert_eq!(
        env.events().all(),
        std::vec![ProjectClaimFeeUpdated {
            project_address: project.clone(),
            project_claim_fee: 450
        }
        .to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.project_fees(&project).project_claim_fee, 450);

    fixture.client.set_default_remove_fee(&fixture.admin, &550);
    assert_eq!(
        env.events().all(),
        std::vec![DefaultRemoveFeeUpdated { default_remove_fee: 550 }
            .to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.default_remove_fee(), 550);

    fixture.client.set_remove_fee(&fixture.admin, &project, &650);
    assert_eq!(
        env.events().all(),
        std::vec![RemoveFeeUpdated { project_address: project.clone(), remove_fee: 650 }
            .to_xdr(&env, &fixture.client.address)]
    );
    assert_eq!(fixture.client.project_fees(&project).remove_fee, 650);
}
