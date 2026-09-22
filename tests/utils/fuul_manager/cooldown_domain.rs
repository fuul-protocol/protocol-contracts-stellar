use crate::test::{
    constructor_helpers::{Bootstrap, MANAGER_WASM},
    roles_pause_helpers::test_env,
    security_helpers::state,
    *,
};

pub(crate) fn scenario(start: u64, now: u64, cooldown: u64, wasm: bool) {
    assert!(now >= start);
    assert!(cooldown >= 86_400);
    let env = test_env();
    env.ledger().with_mut(|l| l.timestamp = start);
    let input = Bootstrap::new(&env);
    let id = if wasm { env.register(MANAGER_WASM, input.args(&env)) } else { input.register(&env) };
    let manager = FuulManagerClient::new(&env, &id);
    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);
    let project = register_mock_project(&env, &input.admin, 0);
    env.mock_all_auths();
    manager.set_currency_token_limit(&input.admin, &input.accepted, &u(&env, 10));
    if cooldown != 86_400 {
        manager.set_claim_cooldown(&input.admin, &u128::from(cooldown));
    }
    // Deliberately avoid timestamp + 300 at the u64 timestamp boundary.
    let mut check = ClaimCheck {
        project_address: project.address.clone(),
        to: recipient.clone(),
        currency: input.accepted.clone(),
        currency_type: TokenType::StellarAsset,
        amount: 6,
        reason: ClaimReason::AffiliatePayout,
        token_id: u(&env, 0),
        deadline: u(&env, u64::MAX),
        proof: BytesN::from_array(&env, &[121; 32]),
        signers: input.signers.clone(),
    };
    manager.claim(&caller, &vec![&env, check.clone()]);
    assert_eq!(manager.currency_limits(&input.accepted).cumulative_claim_per_cooldown, u(&env, 6));
    env.ledger().with_mut(|l| l.timestamp = now);
    check.amount = 5;
    check.proof = BytesN::from_array(&env, &[122; 32]);
    // Independent elapsed-time oracle: no widened start+period sum here.
    let expired = now - start >= cooldown;
    let before = state(&env, &id);
    let result = manager.try_claim(&caller, &vec![&env, check.clone()]);
    if expired {
        assert_eq!(result, Ok(Ok(())));
        assert_eq!(
            manager.currency_limits(&input.accepted),
            CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, 10),
                cumulative_claim_per_cooldown: u(&env, 5),
                claim_cooldown_period_started: now
            }
        );
        assert_eq!(manager.users_claims(&recipient, &input.accepted), u(&env, 11));
    } else {
        assert_eq!(result, Err(Ok(Error::from_contract_error(6304))));
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &id), before);
        check.amount = 4;
        manager.claim(&caller, &vec![&env, check]);
        assert_eq!(
            manager.currency_limits(&input.accepted),
            CurrencyTokenLimit {
                claim_limit_per_cooldown: u(&env, 10),
                cumulative_claim_per_cooldown: u(&env, 10),
                claim_cooldown_period_started: start
            }
        );
        assert_eq!(manager.users_claims(&recipient, &input.accepted), u(&env, 10));
    }
}
