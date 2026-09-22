#![no_std]

mod contract;
#[path = "../../../../contracts/fuul-project/src/error.rs"]
mod error;
#[path = "../../../../contracts/fuul-project/src/events.rs"]
mod events;
#[path = "../../../../contracts/fuul-project/src/storage.rs"]
mod storage;

pub use contract::{FuulProject, FuulProjectClient};
pub use error::ProjectError;
pub use events::{FundsRemoved, KycRequiredUpdated, ProjectInfoUpdated};
