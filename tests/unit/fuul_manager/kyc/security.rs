use crate::test::{
    constructor_helpers::assert_diagnostic, roles_pause_helpers as auth, security_helpers::state, *,
};
use std::cell::Cell;

// Thread-local, non-ledger instrumentation: rollback cannot hide a Project invocation.
std::thread_local! { static PROJECT_CALLS: Cell<u32> = const { Cell::new(0) }; }

#[contract]
struct KycReachProject;
#[contractimpl]
impl KycReachProject {
    #[allow(clippy::too_many_arguments)]
    pub fn claim(
        e: Env,
        manager: Address,
        to: Address,
        _currency: Address,
        _currency_type: TokenType,
        _amount: i128,
        _token_id: U256,
        _proof: BytesN<32>,
        kyc_registered: bool,
    ) -> ProjectClaimResult {
        manager.require_auth();
        PROJECT_CALLS.with(|calls| calls.set(calls.get() + 1));
        e.storage().instance().set(&symbol_short!("kyc"), &kyc_registered);
        ProjectClaimResult { native_user_claim_fee: 0, fee_collector: to }
    }
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
enum ProviderError {
    Unavailable = 7901,
}

#[contract]
struct UnavailableKyc;
#[contractimpl]
impl UnavailableKyc {
    pub fn is_user_kyc_registered(e: Env, _user: Address) -> bool {
        panic_with_error!(&e, ProviderError::Unavailable);
    }
}

#[test]
fn kyc_some_none_and_repeated_none_emit_events_and_forward_false_to_project() {
    let env = auth::test_env();
    let f = claim_fixture(&env);
    let provider = env.register(MockKyc, (f.recipient.clone(),));
    let false_provider = env.register(MockKyc, (Address::generate(&env),));
    let project = register_mock_project(&env, &f.admin, 0);
    for (index, validator) in
        [Some(provider.clone()), Some(provider), Some(false_provider), None, None]
            .into_iter()
            .enumerate()
    {
        auth::authorize(
            &env,
            &f.client.address,
            &f.admin,
            "set_kyc_validator",
            (&f.admin, &validator).into_val(&env),
        );
        f.client.set_kyc_validator(&f.admin, &validator);
        assert_eq!(
            env.events().all(),
            std::vec![KycValidatorUpdated { validator: validator.clone() }
                .to_xdr(&env, &f.client.address)]
        );
        assert_eq!(f.client.kyc_validator(), validator);
        env.mock_all_auths();
        f.client
            .claim(&f.caller, &vec![&env, claim_check(&env, &f, &project.address, 1, index as u8)]);
        assert_eq!(project.last_manager(), Some(f.client.address.clone()));
        assert_eq!(project.last_recipient(), Some(f.recipient.clone()));
        assert_eq!(project.last_kyc(), index < 2);
    }
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 5));
}

#[test]
fn kyc_provider_failure_and_wrong_interface_roll_back_before_project_invocation() {
    use soroban_sdk::xdr::ScError;
    for wrong_interface in [false, true] {
        let env = auth::test_env();
        let f = claim_fixture(&env);
        let project = env.register(KycReachProject, ());
        // The existing MockProject has no is_user_kyc_registered entry point.
        let provider = if wrong_interface {
            register_mock_project(&env, &f.admin, 0).address
        } else {
            env.register(UnavailableKyc, ())
        };
        f.client.set_kyc_validator(&f.admin, &Some(provider));
        let checks = vec![&env, claim_check(&env, &f, &project, 10, 88)];
        let before = state(&env, &f.client.address);
        let project_before = state(&env, &project);
        PROJECT_CALLS.with(|calls| calls.set(0));
        let result = f.client.try_claim(&f.caller, &checks);
        if wrong_interface {
            assert_eq!(result, Err(Ok(auth::native_auth_error())));
            assert_diagnostic(&env, ScError::Context(ScErrorCode::MissingValue));
        } else {
            assert_eq!(result, Err(Ok(Error::from_contract_error(7901))));
            assert_diagnostic(&env, ScError::Contract(7901));
        }
        assert!(env.events().all().events().is_empty());
        PROJECT_CALLS.with(|calls| assert_eq!(calls.get(), 0));
        assert_eq!(state(&env, &f.client.address), before);
        assert_eq!(state(&env, &project), project_before);
        let working = env.register(MockKyc, (f.recipient.clone(),));
        f.client.set_kyc_validator(&f.admin, &Some(working));
        // Same caller/signer/payload proves auth/setup did not cause the failed-provider result.
        f.client.claim(&f.caller, &checks);
        PROJECT_CALLS.with(|calls| assert_eq!(calls.get(), 1));
        env.as_contract(&project, || {
            assert_eq!(env.storage().instance().get::<_, bool>(&symbol_short!("kyc")), Some(true))
        });
        assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 10));
    }
}
