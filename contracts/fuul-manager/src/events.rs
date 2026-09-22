use fuul_core::{ClaimReason, TokenType};
use soroban_sdk::{contractevent, Address, BytesN, U256};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Paused {
    pub account: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unpaused {
    pub account: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimCooldownUpdated {
    pub period: u128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequiredSignersUpdated {
    pub value: u128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenLimitAdded {
    pub token: Address,
    pub limit: U256,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenLimitUpdated {
    pub token: Address,
    pub limit: U256,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoClaimFeeAddressAdded {
    pub account: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoClaimFeeAddressRemoved {
    pub account: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KycValidatorUpdated {
    #[topic]
    pub validator: Option<Address>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Claimed {
    #[topic]
    pub project_address: Address,
    pub to: Address,
    pub currency: Address,
    pub amount: i128,
    pub currency_type: TokenType,
    pub token_id: U256,
    pub reason: ClaimReason,
    pub proof: BytesN<32>,
}
