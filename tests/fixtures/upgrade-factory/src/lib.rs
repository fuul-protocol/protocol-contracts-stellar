#![no_std]

mod contract;
#[path = "../../../../contracts/fuul-factory/src/error.rs"]
mod error;
#[path = "../../../../contracts/fuul-factory/src/events.rs"]
mod events;
#[path = "../../../../contracts/fuul-factory/src/storage.rs"]
mod storage;

pub use contract::{FuulFactory, FuulFactoryClient};
pub use error::FactoryError;
pub use events::{
    DefaultNativeClaimFeeUpdated, DefaultProjectClaimFeeUpdated, DefaultRemoveFeeUpdated,
    FeeCollectorUpdated, NativeClaimFeeUpdated, ProjectClaimFeeUpdated, ProjectCreated,
    RemoveFeeUpdated,
};
