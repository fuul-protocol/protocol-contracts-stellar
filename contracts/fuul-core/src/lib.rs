#![no_std]

#[cfg(feature = "access-control")]
pub mod access;

#[cfg(feature = "access-control")]
pub mod upgrade;

mod fees;
mod interfaces;
mod storage;
mod types;

pub use fees::{calculate_basis_points, FeeMathError, BASIS_POINTS_DENOMINATOR};
pub use interfaces::{
    FuulFactoryInterface, FuulFactoryInterfaceClient, FuulKycInterface, FuulKycInterfaceClient,
    FuulMultiTokenInterface, FuulMultiTokenInterfaceClient, FuulProjectInterface,
    FuulProjectInterfaceClient,
};
pub use storage::{bump_instance, DAY_IN_LEDGERS, INSTANCE_EXTEND_AMOUNT, INSTANCE_TTL_THRESHOLD};
pub use types::{
    ClaimAuthorization, ClaimCheck, ClaimReason, FeesInformation, ProjectClaimResult, ProjectFees,
    TokenType,
};

#[cfg(test)]
#[path = "../../../tests/unit/fuul_core/mod.rs"]
mod test;
