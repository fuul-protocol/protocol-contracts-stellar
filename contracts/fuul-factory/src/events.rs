use soroban_sdk::{contractevent, Address, String};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectCreated {
    pub project_id: u128,
    #[topic]
    pub deployed_address: Address,
    pub project_info_uri: String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefaultRemoveFeeUpdated {
    pub default_remove_fee: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveFeeUpdated {
    pub project_address: Address,
    pub remove_fee: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeCollectorUpdated {
    #[topic]
    pub new_collector: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefaultNativeClaimFeeUpdated {
    pub new_default_native_claim_fee: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeClaimFeeUpdated {
    pub project_address: Address,
    pub native_claim_fee: i128,
}

// The snake-case name exceeds the 32-character symbol limit.
#[contractevent(topics = ["DefaultProjectClaimFeeUpdated"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefaultProjectClaimFeeUpdated {
    pub new_project_claim_fee: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectClaimFeeUpdated {
    pub project_address: Address,
    pub project_claim_fee: u32,
}
