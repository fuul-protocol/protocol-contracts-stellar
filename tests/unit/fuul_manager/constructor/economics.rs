use crate::test::{constructor_helpers::Bootstrap, roles_pause_helpers::test_env, *};

const FUNGIBLE_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_e2e_fungible_fixture.wasm"
));

#[test]
fn explicit_accepted_limit_preserves_the_configured_quantity_without_changing_native_units() {
    for accepted_limit in [
        100_000_000_000_i128,
        1_000_000_000_000,
        100_000_000_000_000_000_000_000,
        1_000_000_000_000_000_000_000_000,
    ] {
        let env = test_env();
        let input = Bootstrap::new(&env);
        let mut args = input.args(&env);
        args.set(8, u(&env, accepted_limit).into_val(&env));
        let id = env.register(FuulManager, args);
        let manager = FuulManagerClient::new(&env, &id);
        assert_eq!(
            manager.currency_limits(&input.accepted).claim_limit_per_cooldown,
            u(&env, accepted_limit)
        );
        assert_eq!(
            manager.currency_limits(&input.native).claim_limit_per_cooldown,
            u(&env, 1_000_000_000_000_i128)
        );
        assert_eq!(manager.users_claims(&input.admin, &input.accepted), u(&env, 0));
        assert!(!manager.paused());
    }
}

#[test]
fn configured_limits_and_real_wasm_token_transfers_preserve_six_seven_and_eighteen_decimals() {
    for decimals in [6_u32, 7, 18] {
        let env = test_env();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let mut input = Bootstrap::new(&env);
        input.accepted = env.register(FUNGIBLE_WASM, (&input.admin, decimals));
        input.native = env
            .deployer()
            .with_stellar_asset(soroban_sdk::Bytes::from_array(&env, &[0; 4]))
            .deploy();
        let unit = 10_i128.pow(decimals);
        let mut args = input.args(&env);
        args.set(8, u(&env, 100_000 * unit).into_val(&env));
        let manager_id = env.register(constructor_helpers::MANAGER_WASM, args);
        let manager = FuulManagerClient::new(&env, &manager_id);
        let token = TokenClient::new(&env, &input.accepted);
        assert_eq!(token.decimals(), decimals);
        assert_eq!(
            manager.currency_limits(&input.accepted).claim_limit_per_cooldown,
            u(&env, 100_000 * unit)
        );
        assert_eq!(
            manager.currency_limits(&input.native).claim_limit_per_cooldown,
            u(&env, 1_000_000_000_000_i128)
        );
        let collector = Address::generate(&env);
        let recipient = Address::generate(&env);
        let caller = Address::generate(&env);
        let factory = env.register(
            FuulFactory,
            (
                &input.admin,
                &manager_id,
                &collector,
                env.deployer().upload_contract_wasm(PROJECT_WASM),
            ),
        );
        env.mock_all_auths();
        let project = FuulFactoryClient::new(&env, &factory).create_fuul_project(
            &input.admin,
            &String::from_str(&env, "ipfs://units"),
            &false,
        );
        StellarAssetClient::new(&env, &input.accepted).mint(&project, &(101 * unit));
        manager.add_no_claim_fee_address(&input.admin, &caller);
        manager.claim(
            &caller,
            &vec![
                &env,
                ClaimCheck {
                    project_address: project.clone(),
                    to: recipient.clone(),
                    currency: input.accepted.clone(),
                    currency_type: TokenType::StellarAsset,
                    amount: 100 * unit,
                    reason: ClaimReason::AffiliatePayout,
                    token_id: u(&env, 0),
                    deadline: u(&env, 1_000_300),
                    proof: BytesN::from_array(&env, &[decimals as u8; 32]),
                    signers: input.signers.clone(),
                },
            ],
        );
        assert_eq!(token.balance(&project), 0);
        assert_eq!(token.balance(&recipient), 100 * unit);
        assert_eq!(token.balance(&collector), unit);
        assert_eq!(manager.users_claims(&recipient, &input.accepted), u(&env, 100 * unit));
        assert_eq!(token.decimals(), decimals);
        env.set_auths(&[]);
        let before = env.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            token.try_transfer(&recipient, MuxedAddress::from(&collector), &unit),
            Err(Ok(roles_pause_helpers::native_auth_error()))
        );
        roles_pause_helpers::assert_auth_failure(&env, 0);
        assert_eq!(env.to_ledger_snapshot().ledger_entries, before);
        assert!(env.events().all().events().is_empty());
    }
}
