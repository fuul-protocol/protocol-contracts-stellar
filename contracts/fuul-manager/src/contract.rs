use fuul_core::{
    access::{default_admin_role, require_admin, FuulAccessControl},
    bump_instance, ClaimCheck, FuulKycInterfaceClient, FuulProjectInterfaceClient,
};
use soroban_sdk::{
    contract, contractimpl, panic_with_error, token::TokenClient, Address, BytesN, Env, IntoVal,
    Map, MuxedAddress, Symbol, Vec, U256,
};
use stellar_access::access_control as oz;
use stellar_contract_utils::pausable::{paused as is_paused, when_not_paused, Pausable};

use crate::{
    error::ManagerError,
    events::{
        ClaimCooldownUpdated, Claimed, KycValidatorUpdated, NoClaimFeeAddressAdded,
        NoClaimFeeAddressRemoved, RequiredSignersUpdated, TokenLimitAdded, TokenLimitUpdated,
    },
    pausable, storage,
    types::CurrencyTokenLimit,
};

pub const MIN_CLAIM_COOLDOWN: u128 = 86_400;
pub const INITIAL_NATIVE_CURRENCY_LIMIT: u128 = 1_000_000_000_000;
pub const MAX_REQUIRED_SIGNERS: u128 = (1_u128 << 96) - 1;

fn pauser_role_symbol(e: &Env) -> Symbol {
    Symbol::new(e, "pauser")
}

fn unpauser_role_symbol(e: &Env) -> Symbol {
    Symbol::new(e, "unpauser")
}

fn claim_signer_role_symbol(e: &Env) -> Symbol {
    Symbol::new(e, "claim_signer")
}

fn validate_signers(e: &Env, required_signers: u128, signers: &Vec<Address>) {
    if required_signers == 0
        || required_signers > MAX_REQUIRED_SIGNERS
        || signers.is_empty()
        || required_signers > u128::from(signers.len())
    {
        panic_with_error!(e, ManagerError::InvalidArgument);
    }

    let mut seen = Map::<Address, bool>::new(e);
    for signer in signers.iter() {
        if seen.contains_key(signer.clone()) {
            panic_with_error!(e, ManagerError::DuplicateSigner);
        }
        seen.set(signer, true);
    }
}

fn add_currency_limit_internal(e: &Env, token: &Address, limit: U256) {
    if limit == U256::from_u32(e, 0) {
        panic_with_error!(e, ManagerError::InvalidArgument);
    }
    if storage::currency_limits(e, token).claim_limit_per_cooldown != U256::from_u32(e, 0) {
        panic_with_error!(e, ManagerError::LimitAlreadySet);
    }

    storage::set_currency_limit(
        e,
        token,
        &CurrencyTokenLimit {
            claim_limit_per_cooldown: limit.clone(),
            cumulative_claim_per_cooldown: U256::from_u32(e, 0),
            claim_cooldown_period_started: e.ledger().timestamp(),
        },
    );
    TokenLimitAdded { token: token.clone(), limit }.publish(e);
}

fn update_currency_limit(e: &Env, currency: &Address, amount: i128) {
    if amount < 0 {
        panic_with_error!(e, ManagerError::InvalidArgument);
    }

    let amount = U256::from_u128(e, amount as u128);
    let mut limit = storage::currency_limits(e, currency);
    if amount > limit.claim_limit_per_cooldown {
        panic_with_error!(e, ManagerError::OverTheLimit);
    }

    let now = e.ledger().timestamp();
    let start = limit.claim_cooldown_period_started;
    if now < start || u128::from(now - start) < storage::claim_cooldown(e) {
        limit.cumulative_claim_per_cooldown =
            checked_total(e, &limit.cumulative_claim_per_cooldown, &amount);
        if limit.cumulative_claim_per_cooldown > limit.claim_limit_per_cooldown {
            panic_with_error!(e, ManagerError::OverTheLimit);
        }
    } else {
        limit.cumulative_claim_per_cooldown = amount;
        limit.claim_cooldown_period_started = e.ledger().timestamp();
    }
    storage::set_currency_limit(e, currency, &limit);
}

fn checked_total(e: &Env, total: &U256, amount: &U256) -> U256 {
    total.checked_add(amount).unwrap_or_else(|| panic_with_error!(e, ManagerError::AmountOverflow))
}

fn validate_claim_signers(e: &Env, claim: &ClaimCheck) {
    if u128::from(claim.signers.len()) < storage::required_signers(e) {
        panic_with_error!(e, ManagerError::NotEnoughSigners);
    }

    let mut seen = Map::<Address, bool>::new(e);
    for signer in claim.signers.iter() {
        if oz::has_role(e, &signer, &claim_signer_role_symbol(e)).is_none() {
            panic_with_error!(e, ManagerError::InvalidSigner);
        }
        if seen.contains_key(signer.clone()) {
            panic_with_error!(e, ManagerError::DuplicateSigner);
        }
        seen.set(signer.clone(), true);
    }
}

fn require_claim_authorizations(e: &Env, claim: &ClaimCheck) {
    let args: Vec<_> = (claim.authorization(),).into_val(e);
    for signer in claim.signers.iter() {
        // Recording mode associates a root with an address object handle.
        // Fresh equivalent handles keep each proof independent, including caller overlap.
        Address::from_string(&signer.to_string()).require_auth_for_args(args.clone());
    }
}

fn is_kyc_registered(e: &Env, user: &Address) -> bool {
    storage::kyc_validator(e).is_some_and(|validator| {
        FuulKycInterfaceClient::new(e, &validator).is_user_kyc_registered(user)
    })
}

#[contract]
pub struct FuulManager;

#[contractimpl]
impl FuulManager {
    #[allow(clippy::too_many_arguments)]
    pub fn __constructor(
        e: &Env,
        admin: Address,
        pauser: Address,
        unpauser: Address,
        initial_required_signers: u128,
        claim_signers: Vec<Address>,
        accepted_currency: Address,
        native_asset: Address,
        initial_kyc_validator: Option<Address>,
        initial_currency_limit: U256,
    ) {
        validate_signers(e, initial_required_signers, &claim_signers);
        if accepted_currency == native_asset {
            panic_with_error!(e, ManagerError::LimitAlreadySet);
        }

        oz::grant_role_no_auth(e, &admin, &default_admin_role(e), &admin);
        oz::grant_role_no_auth(e, &pauser, &pauser_role_symbol(e), &admin);
        oz::grant_role_no_auth(e, &unpauser, &unpauser_role_symbol(e), &admin);
        for signer in claim_signers.iter() {
            oz::grant_role_no_auth(e, &signer, &claim_signer_role_symbol(e), &admin);
        }

        storage::initialize(
            e,
            MIN_CLAIM_COOLDOWN,
            initial_required_signers,
            &initial_kyc_validator,
            &native_asset,
        );
        add_currency_limit_internal(e, &accepted_currency, initial_currency_limit);
        add_currency_limit_internal(
            e,
            &native_asset,
            U256::from_u128(e, INITIAL_NATIVE_CURRENCY_LIMIT),
        );
    }

    pub fn pauser_role(e: &Env) -> Symbol {
        pauser_role_symbol(e)
    }

    pub fn unpauser_role(e: &Env) -> Symbol {
        unpauser_role_symbol(e)
    }

    pub fn claim_signer_role(e: &Env) -> Symbol {
        claim_signer_role_symbol(e)
    }

    pub fn min_claim_cooldown(_e: &Env) -> u128 {
        MIN_CLAIM_COOLDOWN
    }

    pub fn claim_cooldown(e: &Env) -> u128 {
        storage::claim_cooldown(e)
    }

    pub fn required_signers(e: &Env) -> u128 {
        storage::required_signers(e)
    }

    pub fn kyc_validator(e: &Env) -> Option<Address> {
        storage::kyc_validator(e)
    }

    pub fn native_asset(e: &Env) -> Address {
        storage::native_asset(e)
    }

    pub fn currency_limits(e: &Env, token: Address) -> CurrencyTokenLimit {
        storage::currency_limits(e, &token)
    }

    pub fn no_claim_fee_addresses(e: &Env, account: Address) -> bool {
        storage::no_claim_fee_addresses(e, &account)
    }

    pub fn users_claims(e: &Env, user: Address, currency: Address) -> U256 {
        storage::users_claims(e, &user, &currency)
    }

    pub fn claim(e: &Env, caller: Address, checks: Vec<ClaimCheck>) {
        when_not_paused(e);

        caller.require_auth();
        let mut total_native_claim_fee = 0_i128;
        let mut fee_collector = None;

        for check in checks.iter() {
            update_currency_limit(e, &check.currency, check.amount);
            if U256::from_u128(e, u128::from(e.ledger().timestamp())) > check.deadline {
                panic_with_error!(e, ManagerError::DeadlineExpired);
            }
            validate_claim_signers(e, &check);
            require_claim_authorizations(e, &check);

            let cumulative = checked_total(
                e,
                &storage::users_claims(e, &check.to, &check.currency),
                &U256::from_u128(e, check.amount as u128),
            );
            storage::set_users_claims(e, &check.to, &check.currency, &cumulative);

            let kyc_registered = is_kyc_registered(e, &check.to);
            let manager = e.current_contract_address();
            let result = FuulProjectInterfaceClient::new(e, &check.project_address).claim(
                &manager,
                &check.to,
                &check.currency,
                &check.currency_type,
                &check.amount,
                &check.token_id,
                &check.proof,
                &kyc_registered,
            );

            Claimed {
                project_address: check.project_address,
                to: check.to,
                currency: check.currency,
                amount: check.amount,
                currency_type: check.currency_type,
                token_id: check.token_id,
                reason: check.reason,
                proof: check.proof,
            }
            .publish(e);

            if result.native_user_claim_fee < 0 {
                panic_with_error!(e, ManagerError::IncorrectFee);
            }
            fee_collector = Some(result.fee_collector);
            if !storage::no_claim_fee_addresses(e, &caller) {
                total_native_claim_fee = total_native_claim_fee
                    .checked_add(result.native_user_claim_fee)
                    .unwrap_or_else(|| panic_with_error!(e, ManagerError::AmountOverflow));
            }
        }

        // The payer's invocation authorization covers the exact final transfer.
        if total_native_claim_fee > 0 {
            let native_asset = storage::native_asset(e);
            let collector = fee_collector.expect("positive fees require a project result");
            TokenClient::new(e, &native_asset).transfer(
                &caller,
                MuxedAddress::from(&collector),
                &total_native_claim_fee,
            );
        }
    }

    pub fn set_claim_cooldown(e: &Env, caller: Address, period: u128) {
        require_admin(e, &caller);
        if period < MIN_CLAIM_COOLDOWN || period == storage::claim_cooldown(e) {
            panic_with_error!(e, ManagerError::InvalidArgument);
        }
        storage::set_claim_cooldown(e, period);
        ClaimCooldownUpdated { period }.publish(e);
    }

    pub fn set_required_signers(e: &Env, caller: Address, value: u128) {
        require_admin(e, &caller);
        if value == 0 || value > MAX_REQUIRED_SIGNERS || value == storage::required_signers(e) {
            panic_with_error!(e, ManagerError::InvalidArgument);
        }
        storage::set_required_signers(e, value);
        RequiredSignersUpdated { value }.publish(e);
    }

    pub fn add_currency_limit(e: &Env, caller: Address, token: Address, limit: U256) {
        require_admin(e, &caller);
        add_currency_limit_internal(e, &token, limit);
    }

    pub fn set_currency_token_limit(e: &Env, caller: Address, token: Address, limit: U256) {
        require_admin(e, &caller);
        let Some(mut current) = storage::get_currency_limit(e, &token) else {
            panic_with_error!(e, ManagerError::InvalidArgument);
        };
        if current.claim_limit_per_cooldown == U256::from_u32(e, 0)
            || limit == current.claim_limit_per_cooldown
        {
            panic_with_error!(e, ManagerError::InvalidArgument);
        }

        current.claim_limit_per_cooldown = limit.clone();
        storage::set_currency_limit(e, &token, &current);
        TokenLimitUpdated { token, limit }.publish(e);
    }

    pub fn add_no_claim_fee_address(e: &Env, caller: Address, account: Address) {
        require_admin(e, &caller);
        if storage::no_claim_fee_addresses(e, &account) {
            panic_with_error!(e, ManagerError::InvalidArgument);
        }
        storage::set_no_claim_fee_address(e, &account, true);
        NoClaimFeeAddressAdded { account }.publish(e);
    }

    pub fn remove_no_claim_fee_address(e: &Env, caller: Address, account: Address) {
        require_admin(e, &caller);
        if !storage::no_claim_fee_addresses(e, &account) {
            panic_with_error!(e, ManagerError::InvalidArgument);
        }
        storage::set_no_claim_fee_address(e, &account, false);
        NoClaimFeeAddressRemoved { account }.publish(e);
    }

    pub fn set_kyc_validator(e: &Env, caller: Address, validator: Option<Address>) {
        require_admin(e, &caller);
        storage::set_kyc_validator(e, &validator);
        KycValidatorUpdated { validator }.publish(e);
    }

    pub fn keep_alive(e: &Env) {
        bump_instance(e);
    }
}

#[contractimpl(contracttrait)]
impl FuulAccessControl for FuulManager {}

#[contractimpl(contracttrait)]
impl Pausable for FuulManager {
    fn paused(e: &Env) -> bool {
        bump_instance(e);
        is_paused(e)
    }

    fn pause(e: &Env, caller: Address) {
        caller.require_auth();
        oz::ensure_role(e, &pauser_role_symbol(e), &caller);
        pausable::pause(e, caller);
        bump_instance(e);
    }

    fn unpause(e: &Env, caller: Address) {
        caller.require_auth();
        oz::ensure_role(e, &unpauser_role_symbol(e), &caller);
        pausable::unpause(e, caller);
        bump_instance(e);
    }
}

#[contractimpl]
impl fuul_core::upgrade::Upgradeable for FuulManager {
    fn upgrade(e: &Env, new_wasm_hash: BytesN<32>, operator: Address) {
        fuul_core::upgrade::upgrade(e, new_wasm_hash, operator);
    }
}
