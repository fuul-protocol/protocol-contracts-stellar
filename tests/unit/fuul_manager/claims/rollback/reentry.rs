use crate::test::{
    constructor_helpers::assert_diagnostic, roles_pause_helpers as auth, security_helpers::state, *,
};
use std::cell::Cell;
std::thread_local! { static CALLBACKS: Cell<u32> = const { Cell::new(0) }; }

#[contract]
struct ObservedCallback;
#[contractimpl]
impl ObservedCallback {
    pub fn enabled(e: Env, enabled: bool) {
        e.storage().instance().set(&symbol_short!("enabled"), &enabled);
    }
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
        _kyc: bool,
    ) -> ProjectClaimResult {
        manager.require_auth();
        if e.storage().instance().get(&symbol_short!("enabled")).unwrap_or(false) {
            CALLBACKS.with(|n| n.set(n.get() + 1));
            FuulManagerClient::new(&e, &manager).claim(&to, &Vec::new(&e));
        }
        ProjectClaimResult { native_user_claim_fee: 0, fee_collector: to }
    }
}

pub(super) fn assert_callback_rejection_and_control() {
    let env = auth::test_env();
    let f = claim_fixture(&env);
    let earlier = register_mock_project(&env, &f.admin, 0);
    let callback = env.register(ObservedCallback, ());
    let callback_client = ObservedCallbackClient::new(&env, &callback);
    let checks = vec![
        &env,
        claim_check(&env, &f, &earlier.address, 10, 38),
        claim_check(&env, &f, &callback, 55, 39),
    ];
    callback_client.enabled(&true);
    CALLBACKS.with(|n| n.set(0));
    let before = state(&env, &f.client.address);
    let earlier_before = state(&env, &earlier.address);
    assert_eq!(f.client.try_claim(&f.caller, &checks), Err(Ok(auth::native_auth_error())));
    assert_diagnostic(&env, soroban_sdk::xdr::ScError::Context(ScErrorCode::InvalidAction));
    let diagnostics = env.host().get_diagnostic_events().unwrap();
    assert!(std::format!("{diagnostics:?}").contains("Contract re-entry is not allowed"));
    CALLBACKS.with(|n| assert_eq!(n.get(), 1));
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &f.client.address), before);
    assert_eq!(state(&env, &earlier.address), earlier_before);
    assert_eq!(earlier.last_recipient(), None);
    callback_client.enabled(&false);
    f.client.claim(&f.caller, &checks);
    CALLBACKS.with(|n| assert_eq!(n.get(), 1));
    assert_eq!(earlier.last_recipient(), Some(f.recipient.clone()));
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 65));
}
