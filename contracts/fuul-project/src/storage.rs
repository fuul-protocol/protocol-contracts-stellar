use fuul_core::{bump_instance, INSTANCE_EXTEND_AMOUNT, INSTANCE_TTL_THRESHOLD};
use soroban_sdk::{contracttype, Address, BytesN, Env, String};

#[contracttype]
enum DataKey {
    Factory,
    ProjectUri,
    KycRequired,
    ClaimedProof(BytesN<32>),
}

pub fn initialize(e: &Env, factory: &Address, project_uri: &String, kyc_required: bool) {
    e.storage().instance().set(&DataKey::Factory, factory);
    e.storage().instance().set(&DataKey::ProjectUri, project_uri);
    e.storage().instance().set(&DataKey::KycRequired, &kyc_required);
    bump_instance(e);
}

pub fn factory(e: &Env) -> Address {
    bump_instance(e);
    e.storage().instance().get(&DataKey::Factory).expect("factory must be initialized")
}

pub fn project_info_uri(e: &Env) -> String {
    bump_instance(e);
    e.storage().instance().get(&DataKey::ProjectUri).expect("project URI must be initialized")
}

pub fn set_project_uri(e: &Env, project_uri: &String) {
    e.storage().instance().set(&DataKey::ProjectUri, project_uri);
    bump_instance(e);
}

pub fn kyc_required(e: &Env) -> bool {
    bump_instance(e);
    e.storage().instance().get(&DataKey::KycRequired).unwrap_or(false)
}

pub fn set_kyc_required(e: &Env, required: bool) {
    e.storage().instance().set(&DataKey::KycRequired, &required);
    bump_instance(e);
}

pub fn claimed_proofs(e: &Env, proof: &BytesN<32>) -> bool {
    let key = DataKey::ClaimedProof(proof.clone());
    let value = e.storage().persistent().get(&key).unwrap_or(false);
    if value {
        e.storage().persistent().extend_ttl(&key, INSTANCE_TTL_THRESHOLD, INSTANCE_EXTEND_AMOUNT);
    }
    value
}

pub fn set_claimed_proof(e: &Env, proof: &BytesN<32>) {
    let key = DataKey::ClaimedProof(proof.clone());
    e.storage().persistent().set(&key, &true);
    e.storage().persistent().extend_ttl(&key, INSTANCE_TTL_THRESHOLD, INSTANCE_EXTEND_AMOUNT);
}
