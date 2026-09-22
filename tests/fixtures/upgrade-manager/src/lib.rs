#![no_std]

mod contract;
#[path = "../../../../contracts/fuul-manager/src/error.rs"]
mod error;
#[path = "../../../../contracts/fuul-manager/src/events.rs"]
mod events;
#[path = "../../../../contracts/fuul-manager/src/pausable.rs"]
mod pausable;
#[path = "../../../../contracts/fuul-manager/src/storage.rs"]
mod storage;
#[path = "../../../../contracts/fuul-manager/src/types.rs"]
mod types;

pub use contract::{FuulManager, FuulManagerClient};
pub use error::ManagerError;
pub use events::{
    ClaimCooldownUpdated, Claimed, KycValidatorUpdated, NoClaimFeeAddressAdded,
    NoClaimFeeAddressRemoved, Paused, RequiredSignersUpdated, TokenLimitAdded, TokenLimitUpdated,
    Unpaused,
};
pub use types::CurrencyTokenLimit;
