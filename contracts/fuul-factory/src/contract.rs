use fuul_core::{
    access::{default_admin_role, require_admin, FuulAccessControl},
    bump_instance, FeesInformation, ProjectFees, BASIS_POINTS_DENOMINATOR,
};
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, panic_with_error, vec, Address, Bytes, BytesN, Env, IntoVal, String,
    Symbol, Vec,
};
use stellar_access::access_control as oz;

use crate::{
    error::FactoryError,
    events::{
        DefaultNativeClaimFeeUpdated, DefaultProjectClaimFeeUpdated, DefaultRemoveFeeUpdated,
        FeeCollectorUpdated, NativeClaimFeeUpdated, ProjectClaimFeeUpdated, ProjectCreated,
        RemoveFeeUpdated,
    },
    storage,
};

pub const DEFAULT_NATIVE_CLAIM_FEE: i128 = 20_000;
pub const DEFAULT_PROJECT_CLAIM_FEE_BPS: u32 = 100;
pub const DEFAULT_REMOVE_FEE_BPS: u32 = 0;

fn manager_role_symbol(e: &Env) -> Symbol {
    Symbol::new(e, "manager")
}

fn require_changed_i128(e: &Env, current: i128, value: i128) {
    if value < 0 || value == current {
        panic_with_error!(e, FactoryError::InvalidArgument);
    }
}

fn require_changed_bps(e: &Env, current: u32, value: u32) {
    if value == current || value > BASIS_POINTS_DENOMINATOR {
        panic_with_error!(e, FactoryError::InvalidArgument);
    }
}

fn project_salt(e: &Env, tracker: u128) -> BytesN<32> {
    let bytes = if let Ok(narrow) = u64::try_from(tracker) {
        Bytes::from_array(e, &narrow.to_be_bytes())
    } else {
        Bytes::from_array(e, &tracker.to_be_bytes())
    };
    e.crypto().sha256(&bytes).into()
}

#[contract]
pub struct FuulFactory;

#[contractimpl]
impl FuulFactory {
    pub fn __constructor(
        e: &Env,
        admin: Address,
        manager: Address,
        fee_collector: Address,
        project_wasm_hash: BytesN<32>,
    ) {
        oz::grant_role_no_auth(e, &admin, &default_admin_role(e), &admin);
        oz::grant_role_no_auth(e, &manager, &manager_role_symbol(e), &admin);
        storage::initialize(
            e,
            &project_wasm_hash,
            &fee_collector,
            DEFAULT_NATIVE_CLAIM_FEE,
            DEFAULT_PROJECT_CLAIM_FEE_BPS,
            DEFAULT_REMOVE_FEE_BPS,
        );
    }

    pub fn create_fuul_project(
        e: &Env,
        project_admin: Address,
        project_info_uri: String,
        kyc_required: bool,
    ) -> Address {
        if project_info_uri.is_empty() {
            panic_with_error!(e, FactoryError::EmptyUri);
        }

        let tracker = storage::contract_tracker(e);
        let factory = e.current_contract_address();
        let deployer = e.deployer().with_current_contract(project_salt(e, tracker));
        let project_address = deployer.deployed_address();
        let constructor_args = (factory, project_admin, project_info_uri.clone(), kyc_required);
        e.authorize_as_current_contract(vec![
            e,
            InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: project_address,
                    fn_name: Symbol::new(e, "__constructor"),
                    args: constructor_args.clone().into_val(e),
                },
                sub_invocations: Vec::new(e),
            }),
        ]);
        let project = deployer.deploy_v2(storage::project_wasm_hash(e), constructor_args);

        storage::set_project_fees(e, &project, &storage::default_project_fees(e));
        let project_id = (tracker + 1) & storage::MAX_CONTRACT_TRACKER;
        storage::set_contract_tracker(e, project_id);
        ProjectCreated {
            project_id,
            deployed_address: project.clone(),
            project_info_uri: project_info_uri.clone(),
        }
        .publish(e);
        project
    }

    pub fn manager_role(e: &Env) -> Symbol {
        manager_role_symbol(e)
    }

    pub fn has_manager_role(e: &Env, account: Address) -> bool {
        oz::has_role(e, &account, &manager_role_symbol(e)).is_some()
    }

    pub fn contract_tracker(e: &Env) -> u128 {
        storage::contract_tracker(e)
    }

    pub fn project_wasm_hash(e: &Env) -> BytesN<32> {
        storage::project_wasm_hash(e)
    }

    pub fn fee_collector(e: &Env) -> Address {
        storage::fee_collector(e)
    }

    pub fn default_native_claim_fee(e: &Env) -> i128 {
        storage::default_native_claim_fee(e)
    }

    pub fn default_project_claim_fee(e: &Env) -> u32 {
        storage::default_project_claim_fee(e)
    }

    pub fn default_remove_fee(e: &Env) -> u32 {
        storage::default_remove_fee(e)
    }

    pub fn project_fees(e: &Env, project: Address) -> ProjectFees {
        storage::project_fees(e, &project)
    }

    pub fn get_fees_information(e: &Env, project: Address) -> FeesInformation {
        storage::fees_information(e, &project)
    }

    pub fn set_remove_fee(e: &Env, caller: Address, project: Address, value_bps: u32) {
        require_admin(e, &caller);
        let mut fees = storage::project_fees(e, &project);
        require_changed_bps(e, fees.remove_fee, value_bps);
        fees.remove_fee = value_bps;
        storage::set_project_fees(e, &project, &fees);
        RemoveFeeUpdated { project_address: project, remove_fee: value_bps }.publish(e);
    }

    pub fn set_default_remove_fee(e: &Env, caller: Address, value_bps: u32) {
        require_admin(e, &caller);
        require_changed_bps(e, storage::default_remove_fee(e), value_bps);
        storage::set_default_remove_fee(e, value_bps);
        DefaultRemoveFeeUpdated { default_remove_fee: value_bps }.publish(e);
    }

    pub fn set_fee_collector(e: &Env, caller: Address, new_collector: Address) {
        require_admin(e, &caller);
        if new_collector == storage::fee_collector(e) {
            panic_with_error!(e, FactoryError::InvalidArgument);
        }
        storage::set_fee_collector(e, &new_collector);
        FeeCollectorUpdated { new_collector }.publish(e);
    }

    pub fn set_default_native_claim_fee(e: &Env, caller: Address, value: i128) {
        require_admin(e, &caller);
        require_changed_i128(e, storage::default_native_claim_fee(e), value);
        storage::set_default_native_claim_fee(e, value);
        DefaultNativeClaimFeeUpdated { new_default_native_claim_fee: value }.publish(e);
    }

    pub fn set_native_user_claim_fee(e: &Env, caller: Address, project: Address, value: i128) {
        require_admin(e, &caller);
        let mut fees = storage::project_fees(e, &project);
        require_changed_i128(e, fees.native_user_claim_fee, value);
        fees.native_user_claim_fee = value;
        storage::set_project_fees(e, &project, &fees);
        NativeClaimFeeUpdated { project_address: project, native_claim_fee: value }.publish(e);
    }

    pub fn set_project_claim_fee(e: &Env, caller: Address, project: Address, value_bps: u32) {
        require_admin(e, &caller);
        let mut fees = storage::project_fees(e, &project);
        require_changed_bps(e, fees.project_claim_fee, value_bps);
        fees.project_claim_fee = value_bps;
        storage::set_project_fees(e, &project, &fees);
        ProjectClaimFeeUpdated { project_address: project, project_claim_fee: value_bps }
            .publish(e);
    }

    pub fn set_default_project_claim_fee(e: &Env, caller: Address, value_bps: u32) {
        require_admin(e, &caller);
        require_changed_bps(e, storage::default_project_claim_fee(e), value_bps);
        storage::set_default_project_claim_fee(e, value_bps);
        DefaultProjectClaimFeeUpdated { new_project_claim_fee: value_bps }.publish(e);
    }

    pub fn keep_alive(e: &Env) {
        bump_instance(e);
    }
}

#[contractimpl(contracttrait)]
impl FuulAccessControl for FuulFactory {}

#[contractimpl]
impl fuul_core::upgrade::Upgradeable for FuulFactory {
    fn upgrade(e: &Env, new_wasm_hash: BytesN<32>, operator: Address) {
        fuul_core::upgrade::upgrade(e, new_wasm_hash, operator);
    }
}
