use fuul_core::{
    access::{default_admin_role, require_admin, FuulAccessControl, FuulAccessControlClient},
    bump_instance, calculate_basis_points, FuulFactoryInterfaceClient,
    FuulMultiTokenInterfaceClient, ProjectClaimResult, TokenType,
};
use soroban_sdk::{
    contract, contractimpl, panic_with_error, token::TokenClient, Address, BytesN, Env,
    MuxedAddress, String, Symbol, Vec, U256,
};
use stellar_access::access_control as oz;
use stellar_tokens::non_fungible::NonFungibleTokenClient;

use crate::{
    error::ProjectError,
    events::{FundsRemoved, KycRequiredUpdated, ProjectInfoUpdated},
    storage,
};

fn require_non_empty_uri(e: &Env, project_uri: &String) {
    if project_uri.is_empty() {
        panic_with_error!(e, ProjectError::EmptyUri);
    }
}

fn require_non_negative_values(e: &Env, values: &Vec<i128>) {
    for value in values.iter() {
        if value < 0 {
            panic_with_error!(e, ProjectError::InvalidArgument);
        }
    }
}

fn sep50_token_id(e: &Env, token_id: i128) -> u32 {
    token_id.try_into().unwrap_or_else(|_| panic_with_error!(e, ProjectError::InvalidArgument))
}

fn claim_token_id(e: &Env, token_id: &U256) -> u32 {
    token_id
        .to_u128()
        .and_then(|id| u32::try_from(id).ok())
        .unwrap_or_else(|| panic_with_error!(e, ProjectError::InvalidArgument))
}

fn sep50_token_ids(e: &Env, token_ids: &Vec<i128>) -> Vec<u32> {
    let mut converted = Vec::new(e);
    for token_id in token_ids.iter() {
        converted.push_back(sep50_token_id(e, token_id));
    }
    converted
}

#[contract]
pub struct FuulProject;

#[contractimpl]
impl FuulProject {
    pub fn __constructor(
        e: &Env,
        factory: Address,
        project_admin: Address,
        project_uri: String,
        kyc_required: bool,
    ) {
        factory.require_auth();
        require_non_empty_uri(e, &project_uri);
        oz::grant_role_no_auth(e, &project_admin, &default_admin_role(e), &factory);
        storage::initialize(e, &factory, &project_uri, kyc_required);
        ProjectInfoUpdated { project_info_uri: project_uri.clone() }.publish(e);
        KycRequiredUpdated { required: kyc_required }.publish(e);
    }

    pub fn factory(e: &Env) -> Address {
        storage::factory(e)
    }

    pub fn project_info_uri(e: &Env) -> String {
        storage::project_info_uri(e)
    }

    pub fn kyc_required(e: &Env) -> bool {
        storage::kyc_required(e)
    }

    pub fn claimed_proofs(e: &Env, proof: BytesN<32>) -> bool {
        storage::claimed_proofs(e, &proof)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn claim(
        e: &Env,
        manager: Address,
        to: Address,
        currency: Address,
        currency_type: TokenType,
        amount: i128,
        token_id: U256,
        proof: BytesN<32>,
        kyc_registered: bool,
    ) -> ProjectClaimResult {
        manager.require_auth();
        let factory_address = storage::factory(e);
        let factory = FuulFactoryInterfaceClient::new(e, &factory_address);
        if !factory.has_manager_role(&manager) {
            panic_with_error!(e, ProjectError::Unauthorized);
        }
        if amount < 0 {
            panic_with_error!(e, ProjectError::InvalidArgument);
        }
        if storage::claimed_proofs(e, &proof) {
            panic_with_error!(e, ProjectError::ProofAlreadyClaimed);
        }
        storage::set_claimed_proof(e, &proof);

        if storage::kyc_required(e) && !kyc_registered {
            panic_with_error!(e, ProjectError::KycRequiredForClaim);
        }

        let fees_information = factory.get_fees_information(&e.current_contract_address());
        let project = e.current_contract_address();
        match currency_type {
            TokenType::StellarAsset => {
                let fee =
                    calculate_basis_points(e, amount, fees_information.fees.project_claim_fee)
                        .unwrap_or_else(|_| {
                            panic_with_error!(e, ProjectError::FeeCalculationFailed)
                        });
                let token = TokenClient::new(e, &currency);
                token.transfer(&project, MuxedAddress::from(&to), &amount);
                if fee > 0 {
                    token.transfer(
                        &project,
                        MuxedAddress::from(&fees_information.fee_collector),
                        &fee,
                    );
                }
            }
            TokenType::NonFungible => {
                NonFungibleTokenClient::new(e, &currency).transfer(
                    &project,
                    &to,
                    &claim_token_id(e, &token_id),
                );
            }
            TokenType::MultiToken => {
                FuulMultiTokenInterfaceClient::new(e, &currency).transfer(
                    &project,
                    &to,
                    &claim_token_id(e, &token_id),
                    &1,
                );
            }
        }

        ProjectClaimResult {
            native_user_claim_fee: fees_information.fees.native_user_claim_fee,
            fee_collector: fees_information.fee_collector,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn remove_funds(
        e: &Env,
        caller: Address,
        receiver: Address,
        currency: Address,
        currency_type: TokenType,
        amount: i128,
        token_ids: Vec<i128>,
        amounts: Vec<i128>,
    ) {
        require_admin(e, &caller);
        if amount < 0 {
            panic_with_error!(e, ProjectError::InvalidArgument);
        }
        require_non_negative_values(e, &token_ids);
        require_non_negative_values(e, &amounts);

        // Every token type must complete the Factory lookup before any transfer.
        let factory = FuulFactoryInterfaceClient::new(e, &storage::factory(e));
        let fees_information = factory.get_fees_information(&e.current_contract_address());

        match currency_type {
            TokenType::StellarAsset => {
                let fee = calculate_basis_points(e, amount, fees_information.fees.remove_fee)
                    .unwrap_or_else(|_| panic_with_error!(e, ProjectError::FeeCalculationFailed));
                let receiver_amount = amount
                    .checked_sub(fee)
                    .unwrap_or_else(|| panic_with_error!(e, ProjectError::FeeCalculationFailed));
                let project = e.current_contract_address();
                let token = TokenClient::new(e, &currency);
                token.transfer(&project, MuxedAddress::from(&receiver), &receiver_amount);
                // A zero transfer to a collector without a trustline would trap; skip it like `claim`.
                if fee > 0 {
                    token.transfer(
                        &project,
                        MuxedAddress::from(&fees_information.fee_collector),
                        &fee,
                    );
                }
            }
            TokenType::NonFungible => {
                let project = e.current_contract_address();
                let token = NonFungibleTokenClient::new(e, &currency);
                for token_id in sep50_token_ids(e, &token_ids).iter() {
                    token.transfer(&project, &receiver, &token_id);
                }
            }
            TokenType::MultiToken => {
                if token_ids.len() != amounts.len() {
                    panic_with_error!(e, ProjectError::InvalidArgument);
                }
                let project = e.current_contract_address();
                let token = FuulMultiTokenInterfaceClient::new(e, &currency);
                let converted = sep50_token_ids(e, &token_ids);
                for index in 0..converted.len() {
                    token.transfer(
                        &project,
                        &receiver,
                        &converted.get_unchecked(index),
                        &amounts.get_unchecked(index),
                    );
                }
            }
        }

        FundsRemoved { receiver, currency, amount, currency_type, token_ids, amounts }.publish(e);
    }

    pub fn set_project_uri(e: &Env, caller: Address, project_uri: String) {
        require_admin(e, &caller);
        require_non_empty_uri(e, &project_uri);
        storage::set_project_uri(e, &project_uri);
        ProjectInfoUpdated { project_info_uri: project_uri.clone() }.publish(e);
    }

    pub fn set_kyc_required(e: &Env, caller: Address, required: bool) {
        require_admin(e, &caller);
        storage::set_kyc_required(e, required);
        KycRequiredUpdated { required }.publish(e);
    }

    pub fn keep_alive(e: &Env) {
        bump_instance(e);
    }
}

#[contractimpl(contracttrait)]
impl FuulAccessControl for FuulProject {}

#[contractimpl]
impl fuul_core::upgrade::Upgradeable for FuulProject {
    /// Upgrades Project code with Factory administrator authorization.
    fn upgrade(e: &Env, new_wasm_hash: BytesN<32>, operator: Address) {
        operator.require_auth();
        let factory = storage::factory(e);
        if !FuulAccessControlClient::new(e, &factory).has_role(&default_admin_role(e), &operator) {
            panic_with_error!(e, ProjectError::Unauthorized);
        }
        fuul_core::upgrade::upgrade_authorized(e, new_wasm_hash, operator);
    }
}
