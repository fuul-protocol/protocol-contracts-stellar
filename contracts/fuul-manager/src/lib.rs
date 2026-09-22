#![no_std]

mod contract;
mod error;
mod events;
mod pausable;
mod storage;
mod types;

pub use contract::{FuulManager, FuulManagerClient};
pub use error::ManagerError;
pub use events::{
    ClaimCooldownUpdated, Claimed, KycValidatorUpdated, NoClaimFeeAddressAdded,
    NoClaimFeeAddressRemoved, Paused, RequiredSignersUpdated, TokenLimitAdded, TokenLimitUpdated,
    Unpaused,
};
pub use types::CurrencyTokenLimit;

#[cfg(test)]
#[path = "../../../tests/unit/fuul_manager/mod.rs"]
mod test;

#[cfg(test)]
#[path = "../../../tests/regression/mod.rs"]
mod regression;
