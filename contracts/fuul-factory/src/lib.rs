#![no_std]

mod contract;
mod error;
mod events;
mod storage;

pub use contract::{FuulFactory, FuulFactoryClient};
pub use error::FactoryError;
pub use events::{
    DefaultNativeClaimFeeUpdated, DefaultProjectClaimFeeUpdated, DefaultRemoveFeeUpdated,
    FeeCollectorUpdated, NativeClaimFeeUpdated, ProjectClaimFeeUpdated, ProjectCreated,
    RemoveFeeUpdated,
};

#[cfg(test)]
#[path = "../../../tests/unit/fuul_factory/mod.rs"]
mod test;
