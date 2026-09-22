use crate::error::FactoryError;
use fuul_core::{
    bump_instance, FeesInformation, ProjectFees, INSTANCE_EXTEND_AMOUNT, INSTANCE_TTL_THRESHOLD,
};
use soroban_sdk::{contracttype, panic_with_error, Address, BytesN, Env};

pub const MAX_CONTRACT_TRACKER: u128 = (1_u128 << 96) - 1;

fn validate_tracker(e: &Env, value: u128) {
    if value > MAX_CONTRACT_TRACKER {
        panic_with_error!(e, FactoryError::TrackerOverflow);
    }
}

#[contracttype]
enum DataKey {
    ProjectWasmHash,
    ContractTracker,
    FeeCollector,
    DefaultNativeClaimFee,
    DefaultProjectClaimFee,
    DefaultRemoveFee,
    ProjectFees(Address),
}

pub fn initialize(
    e: &Env,
    project_wasm_hash: &BytesN<32>,
    fee_collector: &Address,
    default_native_claim_fee: i128,
    default_project_claim_fee: u32,
    default_remove_fee: u32,
) {
    e.storage().instance().set(&DataKey::ProjectWasmHash, project_wasm_hash);
    e.storage().instance().set(&DataKey::ContractTracker, &0_u128);
    e.storage().instance().set(&DataKey::FeeCollector, fee_collector);
    e.storage().instance().set(&DataKey::DefaultNativeClaimFee, &default_native_claim_fee);
    e.storage().instance().set(&DataKey::DefaultProjectClaimFee, &default_project_claim_fee);
    e.storage().instance().set(&DataKey::DefaultRemoveFee, &default_remove_fee);
    bump_instance(e);
}

pub fn project_wasm_hash(e: &Env) -> BytesN<32> {
    bump_instance(e);
    e.storage()
        .instance()
        .get(&DataKey::ProjectWasmHash)
        .expect("project Wasm hash must be initialized")
}

pub fn contract_tracker(e: &Env) -> u128 {
    bump_instance(e);
    let value = e.storage().instance().get(&DataKey::ContractTracker).unwrap_or(0);
    validate_tracker(e, value);
    value
}

pub fn set_contract_tracker(e: &Env, value: u128) {
    validate_tracker(e, value);
    e.storage().instance().set(&DataKey::ContractTracker, &value);
    bump_instance(e);
}

pub fn fee_collector(e: &Env) -> Address {
    bump_instance(e);
    e.storage().instance().get(&DataKey::FeeCollector).expect("fee collector must be initialized")
}

pub fn set_fee_collector(e: &Env, value: &Address) {
    e.storage().instance().set(&DataKey::FeeCollector, value);
    bump_instance(e);
}

pub fn default_native_claim_fee(e: &Env) -> i128 {
    bump_instance(e);
    e.storage().instance().get(&DataKey::DefaultNativeClaimFee).unwrap_or(0)
}

pub fn set_default_native_claim_fee(e: &Env, value: i128) {
    e.storage().instance().set(&DataKey::DefaultNativeClaimFee, &value);
    bump_instance(e);
}

pub fn default_project_claim_fee(e: &Env) -> u32 {
    bump_instance(e);
    e.storage().instance().get(&DataKey::DefaultProjectClaimFee).unwrap_or(0)
}

pub fn set_default_project_claim_fee(e: &Env, value: u32) {
    e.storage().instance().set(&DataKey::DefaultProjectClaimFee, &value);
    bump_instance(e);
}

pub fn default_remove_fee(e: &Env) -> u32 {
    bump_instance(e);
    e.storage().instance().get(&DataKey::DefaultRemoveFee).unwrap_or(0)
}

pub fn set_default_remove_fee(e: &Env, value: u32) {
    e.storage().instance().set(&DataKey::DefaultRemoveFee, &value);
    bump_instance(e);
}

pub fn default_project_fees(e: &Env) -> ProjectFees {
    ProjectFees {
        native_user_claim_fee: default_native_claim_fee(e),
        project_claim_fee: default_project_claim_fee(e),
        remove_fee: default_remove_fee(e),
    }
}

pub fn project_fees(e: &Env, project: &Address) -> ProjectFees {
    let key = DataKey::ProjectFees(project.clone());
    let value = e.storage().persistent().get(&key);
    if value.is_some() {
        e.storage().persistent().extend_ttl(&key, INSTANCE_TTL_THRESHOLD, INSTANCE_EXTEND_AMOUNT);
    }
    value.unwrap_or(ProjectFees { native_user_claim_fee: 0, project_claim_fee: 0, remove_fee: 0 })
}

pub fn set_project_fees(e: &Env, project: &Address, value: &ProjectFees) {
    let key = DataKey::ProjectFees(project.clone());
    e.storage().persistent().set(&key, value);
    e.storage().persistent().extend_ttl(&key, INSTANCE_TTL_THRESHOLD, INSTANCE_EXTEND_AMOUNT);
}

pub fn fees_information(e: &Env, project: &Address) -> FeesInformation {
    FeesInformation { fee_collector: fee_collector(e), fees: project_fees(e, project) }
}
