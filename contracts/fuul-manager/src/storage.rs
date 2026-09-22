use fuul_core::{bump_instance, INSTANCE_EXTEND_AMOUNT, INSTANCE_TTL_THRESHOLD};
use soroban_sdk::{contracttype, Address, Env, U256};

use crate::types::CurrencyTokenLimit;

#[contracttype]
enum DataKey {
    ClaimCooldown,
    RequiredSigners,
    KycValidator,
    NativeAsset,
    CurrencyLimit(Address),
    FeeExemption(Address),
    UserClaims(Address, Address),
}

fn extend_persistent(e: &Env, key: &DataKey) {
    e.storage().persistent().extend_ttl(key, INSTANCE_TTL_THRESHOLD, INSTANCE_EXTEND_AMOUNT);
}

pub fn initialize(
    e: &Env,
    claim_cooldown: u128,
    required_signers: u128,
    kyc_validator: &Option<Address>,
    native_asset: &Address,
) {
    e.storage().instance().set(&DataKey::ClaimCooldown, &claim_cooldown);
    e.storage().instance().set(&DataKey::RequiredSigners, &required_signers);
    e.storage().instance().set(&DataKey::KycValidator, kyc_validator);
    e.storage().instance().set(&DataKey::NativeAsset, native_asset);
    bump_instance(e);
}

pub fn claim_cooldown(e: &Env) -> u128 {
    bump_instance(e);
    e.storage().instance().get(&DataKey::ClaimCooldown).unwrap_or(0)
}

pub fn set_claim_cooldown(e: &Env, value: u128) {
    e.storage().instance().set(&DataKey::ClaimCooldown, &value);
    bump_instance(e);
}

pub fn required_signers(e: &Env) -> u128 {
    bump_instance(e);
    e.storage().instance().get(&DataKey::RequiredSigners).unwrap_or(0)
}

pub fn set_required_signers(e: &Env, value: u128) {
    e.storage().instance().set(&DataKey::RequiredSigners, &value);
    bump_instance(e);
}

pub fn kyc_validator(e: &Env) -> Option<Address> {
    bump_instance(e);
    e.storage().instance().get(&DataKey::KycValidator).unwrap_or(None)
}

pub fn set_kyc_validator(e: &Env, value: &Option<Address>) {
    e.storage().instance().set(&DataKey::KycValidator, value);
    bump_instance(e);
}

pub fn native_asset(e: &Env) -> Address {
    bump_instance(e);
    e.storage().instance().get(&DataKey::NativeAsset).expect("native asset must be initialized")
}

pub fn get_currency_limit(e: &Env, token: &Address) -> Option<CurrencyTokenLimit> {
    let key = DataKey::CurrencyLimit(token.clone());
    let value = e.storage().persistent().get(&key);
    if value.is_some() {
        extend_persistent(e, &key);
    }
    value
}

pub fn currency_limits(e: &Env, token: &Address) -> CurrencyTokenLimit {
    get_currency_limit(e, token).unwrap_or_else(|| CurrencyTokenLimit::zero(e))
}

pub fn set_currency_limit(e: &Env, token: &Address, value: &CurrencyTokenLimit) {
    let key = DataKey::CurrencyLimit(token.clone());
    e.storage().persistent().set(&key, value);
    extend_persistent(e, &key);
}

pub fn no_claim_fee_addresses(e: &Env, account: &Address) -> bool {
    let key = DataKey::FeeExemption(account.clone());
    let value = e.storage().persistent().get(&key).unwrap_or(false);
    if value {
        extend_persistent(e, &key);
    }
    value
}

pub fn set_no_claim_fee_address(e: &Env, account: &Address, value: bool) {
    let key = DataKey::FeeExemption(account.clone());
    if value {
        e.storage().persistent().set(&key, &true);
        extend_persistent(e, &key);
    } else {
        e.storage().persistent().remove(&key);
    }
}

pub fn users_claims(e: &Env, user: &Address, currency: &Address) -> U256 {
    let key = DataKey::UserClaims(user.clone(), currency.clone());
    let value = e.storage().persistent().get(&key).unwrap_or_else(|| U256::from_u32(e, 0));
    if value != U256::from_u32(e, 0) {
        extend_persistent(e, &key);
    }
    value
}

pub fn set_users_claims(e: &Env, user: &Address, currency: &Address, value: &U256) {
    let key = DataKey::UserClaims(user.clone(), currency.clone());
    e.storage().persistent().set(&key, value);
    extend_persistent(e, &key);
}
