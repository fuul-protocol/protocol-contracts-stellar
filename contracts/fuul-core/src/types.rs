use soroban_sdk::{contracttype, Address, BytesN, Vec, U256};

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimReason {
    AffiliatePayout,
    EndUserPayout,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenType {
    StellarAsset,
    NonFungible,
    MultiToken,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectFees {
    pub native_user_claim_fee: i128,
    pub project_claim_fee: u32,
    pub remove_fee: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeesInformation {
    pub fee_collector: Address,
    pub fees: ProjectFees,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimAuthorization {
    pub project_address: Address,
    pub to: Address,
    pub currency: Address,
    pub amount: i128,
    pub reason: ClaimReason,
    pub token_id: U256,
    pub deadline: U256,
    pub proof: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimCheck {
    pub project_address: Address,
    pub to: Address,
    pub currency: Address,
    pub currency_type: TokenType,
    pub amount: i128,
    pub reason: ClaimReason,
    pub token_id: U256,
    pub deadline: U256,
    pub proof: BytesN<32>,
    pub signers: Vec<Address>,
}

impl ClaimCheck {
    pub fn authorization(&self) -> ClaimAuthorization {
        ClaimAuthorization {
            project_address: self.project_address.clone(),
            to: self.to.clone(),
            currency: self.currency.clone(),
            amount: self.amount,
            reason: self.reason,
            token_id: self.token_id.clone(),
            deadline: self.deadline.clone(),
            proof: self.proof.clone(),
        }
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectClaimResult {
    pub native_user_claim_fee: i128,
    pub fee_collector: Address,
}
