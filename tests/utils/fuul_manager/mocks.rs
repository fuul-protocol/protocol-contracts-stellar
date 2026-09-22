use crate::test::*;

#[contracttype]
pub(super) enum MockProjectKey {
    Result,
    LastKyc,
    LastManager,
    LastRecipient,
}

#[contract]
pub(super) struct MockProject;

#[contractimpl]
impl MockProject {
    pub fn __constructor(e: &Env, result: ProjectClaimResult) {
        e.storage().instance().set(&MockProjectKey::Result, &result);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn claim(
        e: &Env,
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
        e.storage().instance().set(&MockProjectKey::LastKyc, &kyc_registered);
        e.storage().instance().set(&MockProjectKey::LastManager, &manager);
        e.storage().instance().set(&MockProjectKey::LastRecipient, &to);
        e.storage().instance().get(&MockProjectKey::Result).expect("result must be configured")
    }

    pub fn last_kyc(e: &Env) -> bool {
        e.storage().instance().get(&MockProjectKey::LastKyc).unwrap_or(false)
    }

    pub fn last_manager(e: &Env) -> Option<Address> {
        e.storage().instance().get(&MockProjectKey::LastManager)
    }

    pub fn last_recipient(e: &Env) -> Option<Address> {
        e.storage().instance().get(&MockProjectKey::LastRecipient)
    }
}

#[contract]
pub(super) struct ReplayRejectingProject;

#[contractimpl]
impl ReplayRejectingProject {
    #[allow(clippy::too_many_arguments)]
    pub fn claim(
        e: &Env,
        manager: Address,
        _to: Address,
        _currency: Address,
        _currency_type: TokenType,
        _amount: i128,
        _token_id: U256,
        _proof: BytesN<32>,
        _kyc_registered: bool,
    ) -> ProjectClaimResult {
        manager.require_auth();
        panic_with_error!(e, ProjectError::ProofAlreadyClaimed);
    }
}

#[contract]
pub(super) struct KycRejectingProject;

#[contractimpl]
impl KycRejectingProject {
    #[allow(clippy::too_many_arguments)]
    pub fn claim(
        e: &Env,
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
        if !kyc_registered {
            panic_with_error!(e, ProjectError::KycRequiredForClaim);
        }
        ProjectClaimResult { native_user_claim_fee: 0, fee_collector: to }
    }
}

#[contract]
pub(super) struct ReentrantManagerProject;

#[contractimpl]
impl ReentrantManagerProject {
    #[allow(clippy::too_many_arguments)]
    pub fn claim(
        e: &Env,
        manager: Address,
        to: Address,
        _currency: Address,
        _currency_type: TokenType,
        _amount: i128,
        _token_id: U256,
        _proof: BytesN<32>,
        _kyc_registered: bool,
    ) -> ProjectClaimResult {
        manager.require_auth();
        FuulManagerClient::new(e, &manager).claim(&to, &Vec::new(e));
        ProjectClaimResult { native_user_claim_fee: 0, fee_collector: to }
    }
}

#[contract]
pub(super) struct MockKyc;

#[contractimpl]
impl MockKyc {
    pub fn __constructor(e: &Env, registered_user: Address) {
        e.storage().instance().set(&symbol_short!("user"), &registered_user);
    }

    pub fn is_user_kyc_registered(e: &Env, user: Address) -> bool {
        e.storage().instance().get::<_, Address>(&symbol_short!("user")) == Some(user)
    }
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub(super) enum RejectingSmartAccountError {
    AuthorizationAttempted = 7900,
}

#[contract]
pub(super) struct RejectingSmartAccount;

pub(super) static REJECTING_SMART_ACCOUNT_INVOCATIONS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[contractimpl]
impl CustomAccountInterface for RejectingSmartAccount {
    type Signature = ();
    type Error = RejectingSmartAccountError;

    fn __check_auth(
        _env: Env,
        _signature_payload: Hash<32>,
        _signatures: (),
        _auth_contexts: Vec<Context>,
    ) -> Result<(), Self::Error> {
        REJECTING_SMART_ACCOUNT_INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(RejectingSmartAccountError::AuthorizationAttempted)
    }
}

#[contract]
pub(super) struct PanickingKyc;

#[contractimpl]
impl PanickingKyc {
    pub fn is_user_kyc_registered(_env: Env, _user: Address) -> bool {
        panic!("empty claim must not call the KYC validator");
    }
}

#[contract]
pub(super) struct MockNonFungible;

#[contractimpl]
impl MockNonFungible {
    pub fn mint(e: &Env, to: Address, token_id: u32) {
        NonFungibleBase::mint(e, &to, token_id);
    }
}

#[contractimpl(contracttrait)]
impl NonFungibleToken for MockNonFungible {
    type ContractType = NonFungibleBase;
}

pub(super) fn multi_token_balance(e: &Env, owner: &Address, token_id: u32) -> i128 {
    e.storage().persistent().get(&(symbol_short!("balance"), owner.clone(), token_id)).unwrap_or(0)
}

pub(super) fn set_multi_token_balance(e: &Env, owner: &Address, token_id: u32, amount: i128) {
    e.storage().persistent().set(&(symbol_short!("balance"), owner.clone(), token_id), &amount);
}

#[contract]
pub(super) struct MockMultiToken;

#[contractimpl]
impl MockMultiToken {
    pub fn mint(e: &Env, to: Address, token_id: u32, amount: i128) {
        let balance =
            multi_token_balance(e, &to, token_id).checked_add(amount).expect("balance overflow");
        set_multi_token_balance(e, &to, token_id, balance);
    }

    pub fn balance(e: &Env, owner: Address, token_id: u32) -> i128 {
        multi_token_balance(e, &owner, token_id)
    }

    pub fn transfer(e: &Env, from: Address, to: Address, token_id: u32, amount: i128) {
        from.require_auth();
        assert!(amount >= 0, "amount must not be negative");
        let from_balance = multi_token_balance(e, &from, token_id);
        assert!(from_balance >= amount, "insufficient balance");
        if from == to {
            return;
        }
        let to_balance =
            multi_token_balance(e, &to, token_id).checked_add(amount).expect("balance overflow");
        set_multi_token_balance(e, &from, token_id, from_balance - amount);
        set_multi_token_balance(e, &to, token_id, to_balance);
    }
}
