use soroban_sdk::{contractclient, Address, BytesN, Env, U256};

use crate::{FeesInformation, ProjectClaimResult, TokenType};

#[contractclient(name = "FuulFactoryInterfaceClient")]
pub trait FuulFactoryInterface {
    fn has_manager_role(e: Env, account: Address) -> bool;
    fn get_fees_information(e: Env, project: Address) -> FeesInformation;
}

#[contractclient(name = "FuulProjectInterfaceClient")]
pub trait FuulProjectInterface {
    #[allow(clippy::too_many_arguments)]
    fn claim(
        e: Env,
        manager: Address,
        to: Address,
        currency: Address,
        currency_type: TokenType,
        amount: i128,
        token_id: U256,
        proof: BytesN<32>,
        kyc_registered: bool,
    ) -> ProjectClaimResult;
}

#[contractclient(name = "FuulKycInterfaceClient")]
pub trait FuulKycInterface {
    fn is_user_kyc_registered(e: Env, user: Address) -> bool;
}

/// Multi-token transfer interface; implementations must authenticate `from`.
#[contractclient(name = "FuulMultiTokenInterfaceClient")]
pub trait FuulMultiTokenInterface {
    fn transfer(e: Env, from: Address, to: Address, token_id: u32, amount: i128);
}
