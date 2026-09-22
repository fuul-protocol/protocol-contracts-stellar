use crate::test::{
    roles_pause_helpers as auth,
    security_helpers::{authorize_claims, state},
    *,
};
use fuul_core::ProjectFees;
use fuul_factory::FeeCollectorUpdated;

const FACTORY_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_factory.wasm"
));
const MANAGER_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_manager.wasm"
));

#[test]
fn snapshots_overrides_and_live_collector_drive_real_claim_and_removal_settlement() {
    for compiled in [false, true] {
        let e = auth::test_env();
        let mut f = claim_fixture(&e);
        if compiled {
            let manager = e.register(
                MANAGER_WASM,
                (
                    &f.admin,
                    &f.pauser,
                    &f.pauser,
                    1_u128,
                    vec![&e, f.signer.clone()],
                    &f.currency,
                    &f.native_asset,
                    None::<Address>,
                    u(&e, 1_000_000_000_000_i128),
                ),
            );
            f.client = FuulManagerClient::new(&e, &manager);
        }
        let old = Address::generate(&e);
        let new = Address::generate(&e);
        let receiver = Address::generate(&e);
        let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
        let args = (&f.admin, &f.client.address, &old, &code);
        let factory_id =
            if compiled { e.register(FACTORY_WASM, args) } else { e.register(FuulFactory, args) };
        let factory = FuulFactoryClient::new(&e, &factory_id);
        let first = factory.create_fuul_project(
            &f.admin,
            &String::from_str(&e, "ipfs://before-default-change"),
            &false,
        );
        assert_eq!(
            factory.project_fees(&first),
            ProjectFees { native_user_claim_fee: 20_000, project_claim_fee: 100, remove_fee: 0 }
        );

        // Only setup uses unrestricted authorization.
        e.mock_all_auths();
        factory.set_default_native_claim_fee(&f.admin, &30_000);
        factory.set_default_project_claim_fee(&f.admin, &250);
        factory.set_default_remove_fee(&f.admin, &500);
        let second = factory.create_fuul_project(
            &f.admin,
            &String::from_str(&e, "ipfs://new-defaults"),
            &false,
        );
        assert_eq!(
            factory.project_fees(&first),
            ProjectFees { native_user_claim_fee: 20_000, project_claim_fee: 100, remove_fee: 0 }
        );
        factory.set_native_user_claim_fee(&f.admin, &first, &40_000);
        factory.set_project_claim_fee(&f.admin, &first, &300);
        factory.set_remove_fee(&f.admin, &first, &1_000);
        factory.set_default_native_claim_fee(&f.admin, &90_000);
        factory.set_default_project_claim_fee(&f.admin, &9_999);
        factory.set_default_remove_fee(&f.admin, &10_000);
        let future = factory.create_fuul_project(
            &f.admin,
            &String::from_str(&e, "ipfs://future-defaults"),
            &false,
        );
        for (project, fees) in [
            (
                &first,
                ProjectFees {
                    native_user_claim_fee: 40_000,
                    project_claim_fee: 300,
                    remove_fee: 1_000,
                },
            ),
            (
                &second,
                ProjectFees {
                    native_user_claim_fee: 30_000,
                    project_claim_fee: 250,
                    remove_fee: 500,
                },
            ),
            (
                &future,
                ProjectFees {
                    native_user_claim_fee: 90_000,
                    project_claim_fee: 9_999,
                    remove_fee: 10_000,
                },
            ),
        ] {
            assert_eq!(factory.project_fees(project), fees);
        }
        for project in [&first, &second] {
            StellarAssetClient::new(&e, &f.currency).mint(project, &50_000);
        }
        let currency = TokenClient::new(&e, &f.currency);
        let native = TokenClient::new(&e, &f.native_asset);
        let balances = || {
            [
                currency.balance(&first),
                currency.balance(&second),
                currency.balance(&f.recipient),
                currency.balance(&old),
                currency.balance(&new),
                currency.balance(&receiver),
                native.balance(&f.caller),
                native.balance(&old),
                native.balance(&new),
            ]
        };
        let initial = vec![&e, claim_check(&e, &f, &first, 10_000, 71)];
        let old_payment = MockAuthInvoke {
            contract: &f.native_asset,
            fn_name: "transfer",
            args: (&f.caller, MuxedAddress::from(&old), 40_000_i128).into_val(&e),
            sub_invokes: &[],
        };
        authorize_claims(&e, &f.client.address, &f.caller, &initial, &[old_payment]);
        f.client.claim(&f.caller, &initial);
        assert_eq!(balances(), [39_700, 50_000, 10_000, 300, 0, 0, 960_000, 40_000, 0]);

        auth::authorize(
            &e,
            &factory_id,
            &f.admin,
            "set_fee_collector",
            (&f.admin, &new).into_val(&e),
        );
        factory.set_fee_collector(&f.admin, &new);
        assert_eq!(
            e.events().all(),
            std::vec![FeeCollectorUpdated { new_collector: new.clone() }.to_xdr(&e, &factory_id)]
        );
        for project in [&first, &second, &future] {
            let info = factory.get_fees_information(project);
            assert_eq!(info.fee_collector, new);
            assert_eq!(info.fees, factory.project_fees(project));
        }
        let checks = vec![
            &e,
            claim_check(&e, &f, &first, 10_000, 72),
            claim_check(&e, &f, &second, 10_000, 73),
        ];
        let before_balances = balances();
        let before = [
            state(&e, &factory_id),
            state(&e, &f.client.address),
            state(&e, &first),
            state(&e, &second),
        ];
        let wrong_payment = MockAuthInvoke {
            contract: &f.native_asset,
            fn_name: "transfer",
            args: (&f.caller, MuxedAddress::from(&old), 70_000_i128).into_val(&e),
            sub_invokes: &[],
        };
        authorize_claims(&e, &f.client.address, &f.caller, &checks, &[wrong_payment]);
        assert_eq!(f.client.try_claim(&f.caller, &checks), Err(Ok(auth::native_auth_error())));
        auth::assert_auth_failure(&e, 0);
        assert!(e.events().all().events().is_empty());
        assert_eq!(
            [
                state(&e, &factory_id),
                state(&e, &f.client.address),
                state(&e, &first),
                state(&e, &second)
            ],
            before
        );
        assert_eq!(balances(), before_balances);
        for (project, proof) in [(&first, 72), (&second, 73)] {
            assert!(!FuulProjectClient::new(&e, project)
                .claimed_proofs(&BytesN::from_array(&e, &[proof; 32])));
        }
        let new_payment = MockAuthInvoke {
            contract: &f.native_asset,
            fn_name: "transfer",
            args: (&f.caller, MuxedAddress::from(&new), 70_000_i128).into_val(&e),
            sub_invokes: &[],
        };
        authorize_claims(&e, &f.client.address, &f.caller, &checks, &[new_payment]);
        f.client.claim(&f.caller, &checks);
        assert_eq!(e.auths().len(), 3); // payer consent and two independent signer roots
        assert_eq!(e.events().all().filter_by_contract(&f.native_asset).events().len(), 1);
        assert_eq!(balances(), [29_400, 39_750, 30_000, 300, 550, 0, 890_000, 40_000, 70_000]);
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&e, 30_000));
        for (project, proof) in [(&first, 72), (&second, 73)] {
            assert!(FuulProjectClient::new(&e, project)
                .claimed_proofs(&BytesN::from_array(&e, &[proof; 32])));
        }

        for project in [&first, &second] {
            let empty = Vec::<i128>::new(&e);
            let args = (
                &f.admin,
                &receiver,
                &f.currency,
                TokenType::StellarAsset,
                10_000_i128,
                &empty,
                &empty,
            )
                .into_val(&e);
            auth::authorize(&e, project, &f.admin, "remove_funds", args);
            FuulProjectClient::new(&e, project).remove_funds(
                &f.admin,
                &receiver,
                &f.currency,
                &TokenType::StellarAsset,
                &10_000,
                &empty,
                &empty,
            );
        }
        assert_eq!(
            balances(),
            [19_400, 29_750, 30_000, 300, 2_050, 18_500, 890_000, 40_000, 70_000]
        );
    }
}
#[test]
fn project_administrators_are_isolated_from_factory_manager_and_other_projects() {
    use fuul_core::access::FuulAccessControlClient;
    for compiled in [false, true] {
        let e = auth::test_env();
        let f = claim_fixture(&e);
        let factory_admin = Address::generate(&e);
        let factory_args = (
            factory_admin.clone(),
            f.client.address.clone(),
            Address::generate(&e),
            e.deployer().upload_contract_wasm(PROJECT_WASM),
        );
        let factory = if compiled {
            e.register(FACTORY_WASM, factory_args)
        } else {
            e.register(FuulFactory, factory_args)
        };
        let a = Address::generate(&e);
        let b = Address::generate(&e);
        let factory_client = FuulFactoryClient::new(&e, &factory);
        let first =
            factory_client.create_fuul_project(&a, &String::from_str(&e, "ipfs://scope-a"), &false);
        let second =
            factory_client.create_fuul_project(&b, &String::from_str(&e, "ipfs://scope-b"), &false);
        let role = Symbol::new(&e, "default_admin");
        assert!(FuulAccessControlClient::new(&e, &factory).has_role(&role, &factory_admin));
        assert!(FuulAccessControlClient::new(&e, &f.client.address).has_role(&role, &f.admin));
        for (id, admin, other) in [(&first, &a, &b), (&second, &b, &a)] {
            let access = FuulAccessControlClient::new(&e, id);
            assert_eq!(access.get_role_members(&role), vec![&e, admin.clone()]);
            for actor in [&factory_admin, &f.admin, other] {
                assert!(!access.has_role(&role, actor));
                auth::authorize(&e, id, actor, "set_kyc_required", (actor, true).into_val(&e));
                let before = e.to_ledger_snapshot().ledger_entries;
                assert_eq!(
                    FuulProjectClient::new(&e, id).try_set_kyc_required(actor, &true),
                    Err(Ok(Error::from_contract_error(2000)))
                );
                assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
                assert!(e.events().all().events().is_empty());
            }
        }
        auth::authorize(&e, &first, &a, "grant_role", (&role, &b, &a).into_val(&e));
        FuulAccessControlClient::new(&e, &first).grant_role(&role, &b, &a);
        assert!(!FuulAccessControlClient::new(&e, &factory).has_role(&role, &b));
        assert!(!FuulAccessControlClient::new(&e, &f.client.address).has_role(&role, &b));
        auth::authorize(&e, &first, &b, "set_kyc_required", (&b, true).into_val(&e));
        FuulProjectClient::new(&e, &first).set_kyc_required(&b, &true);
        assert!(!FuulProjectClient::new(&e, &second).kyc_required());
    }
}
