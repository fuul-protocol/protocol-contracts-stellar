#![no_std]

mod contract;
mod error;
mod events;
mod storage;

pub use contract::{FuulProject, FuulProjectClient};
pub use error::ProjectError;
pub use events::{FundsRemoved, KycRequiredUpdated, ProjectInfoUpdated};

#[cfg(test)]
#[path = "../../../tests/unit/fuul_project/mod.rs"]
mod test;
