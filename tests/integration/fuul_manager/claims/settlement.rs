use crate::test::{
    roles_pause_helpers as auth,
    security_helpers::{authorize_claims, state},
    *,
};

#[test]
fn real_project_batch_late_settlement_failures_restore_balances_proofs_and_manager_state() {
    for mutation in 0..9 {
        let env = auth::test_env();
        let f = claim_fixture(&env);
        let collector = Address::generate(&env);
        let factory = env.register(
            FuulFactory,
            (
                f.admin.clone(),
                f.client.address.clone(),
                collector.clone(),
                env.deployer().upload_contract_wasm(PROJECT_WASM),
            ),
        );
        let factory_client = FuulFactoryClient::new(&env, &factory);
        let first = factory_client.create_fuul_project(
            &f.admin,
            &String::from_str(&env, "ipfs://first"),
            &false,
        );
        let second = factory_client.create_fuul_project(
            &f.admin,
            &String::from_str(&env, "ipfs://second"),
            &false,
        );
        for project in [&first, &second] {
            StellarAssetClient::new(&env, &f.currency).mint(project, &101_000);
        }
        let checks = vec![
            &env,
            claim_check(&env, &f, &first, 100_000, 101),
            claim_check(&env, &f, &second, 100_000, 102),
        ];
        if mutation == 6 {
            FuulProjectClient::new(&env, &second).set_kyc_required(&f.admin, &true);
        }
        if mutation == 8 {
            factory_client.set_native_user_claim_fee(&f.admin, &first, &i128::MAX);
            factory_client.set_native_user_claim_fee(&f.admin, &second, &1);
        }
        if mutation == 5 {
            TokenClient::new(&env, &f.native_asset).transfer(
                &f.caller,
                MuxedAddress::from(&f.admin),
                &970_000,
            );
        }
        let balances = || {
            [
                TokenClient::new(&env, &f.currency).balance(&first),
                TokenClient::new(&env, &f.currency).balance(&second),
                TokenClient::new(&env, &f.currency).balance(&f.recipient),
                TokenClient::new(&env, &f.currency).balance(&collector),
                TokenClient::new(&env, &f.native_asset).balance(&f.caller),
                TokenClient::new(&env, &f.native_asset).balance(&collector),
            ]
        };
        let before_balances = balances();
        let manager_before = state(&env, &f.client.address);
        let first_before = state(&env, &first);
        let second_before = state(&env, &second);
        let wrong = Address::generate(&env);
        let token = if mutation == 1 { &f.currency } else { &f.native_asset };
        let to = if mutation == 2 { &wrong } else { &collector };
        let amount = if mutation == 3 { 39_999_i128 } else { 40_000_i128 };
        let payer = if mutation == 7 { &f.admin } else { &f.caller };
        let bad_transfer = MockAuthInvoke {
            contract: token,
            fn_name: "transfer",
            args: (payer, MuxedAddress::from(to), amount).into_val(&env),
            sub_invokes: &[],
        };
        let good_transfer = MockAuthInvoke {
            contract: &f.native_asset,
            fn_name: "transfer",
            args: (&f.caller, MuxedAddress::from(&collector), 40_000_i128).into_val(&env),
            sub_invokes: &[],
        };
        let old_transfer = MockAuthInvoke {
            args: (&f.caller, MuxedAddress::from(&collector), 20_000_i128).into_val(&env),
            ..good_transfer.clone()
        };
        let transfers = if mutation == 0 || mutation == 6 || mutation == 8 {
            std::vec![]
        } else if mutation == 4 {
            std::vec![old_transfer.clone(), old_transfer]
        } else {
            std::vec![bad_transfer]
        };
        authorize_claims(&env, &f.client.address, &f.caller, &checks, &transfers);
        let result = f.client.try_claim(&f.caller, &checks);
        if mutation == 5 {
            assert_eq!(result, Err(Ok(Error::from_contract_error(10))));
            constructor_helpers::assert_diagnostic(&env, soroban_sdk::xdr::ScError::Contract(10));
        } else if mutation == 6 {
            assert_eq!(result, Err(Ok(Error::from_contract_error(6103))));
            constructor_helpers::assert_diagnostic(&env, soroban_sdk::xdr::ScError::Contract(6103));
        } else if mutation == 8 {
            assert_eq!(result, Err(Ok(Error::from_contract_error(6308))));
            constructor_helpers::assert_diagnostic(&env, soroban_sdk::xdr::ScError::Contract(6308));
        } else {
            assert_eq!(result, Err(Ok(auth::native_auth_error())), "mutation {mutation}");
            auth::assert_auth_failure(&env, 0);
        }
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &f.client.address), manager_before);
        assert_eq!(state(&env, &first), first_before);
        assert_eq!(state(&env, &second), second_before);
        assert_eq!(balances(), before_balances);
        for (project, proof) in [(&first, 101), (&second, 102)] {
            assert!(!FuulProjectClient::new(&env, project)
                .claimed_proofs(&BytesN::from_array(&env, &[proof; 32])));
        }
        if mutation == 5 {
            env.mock_all_auths();
            StellarAssetClient::new(&env, &f.native_asset).mint(&f.caller, &970_000);
        }
        if mutation == 6 {
            env.mock_all_auths();
            FuulProjectClient::new(&env, &second).set_kyc_required(&f.admin, &false);
        }
        if mutation == 8 {
            env.mock_all_auths();
            factory_client.set_native_user_claim_fee(&f.admin, &first, &20_000);
            factory_client.set_native_user_claim_fee(&f.admin, &second, &20_000);
        }
        let transfers = [good_transfer];
        authorize_claims(&env, &f.client.address, &f.caller, &checks, &transfers);
        f.client.claim(&f.caller, &checks);
        assert_eq!(env.events().all().filter_by_contract(&f.native_asset).events().len(), 1);
        assert_eq!(balances(), [0, 0, 200_000, 2_000, 960_000, 40_000]);
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 200_000));
        for (project, proof) in [(&first, 101), (&second, 102)] {
            assert!(FuulProjectClient::new(&env, project)
                .claimed_proofs(&BytesN::from_array(&env, &[proof; 32])));
        }
    }
}
