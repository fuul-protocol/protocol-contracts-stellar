use fuul_core::TokenType;
use soroban_sdk::{contractevent, Address, String, Vec};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectInfoUpdated {
    pub project_info_uri: String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KycRequiredUpdated {
    pub required: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundsRemoved {
    pub receiver: Address,
    pub currency: Address,
    pub amount: i128,
    pub currency_type: TokenType,
    pub token_ids: Vec<i128>,
    pub amounts: Vec<i128>,
}
