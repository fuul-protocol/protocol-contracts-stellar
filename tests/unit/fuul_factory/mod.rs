extern crate std;

use fuul_core::access::FuulAccessControlClient;
use fuul_core::{FeesInformation, ProjectFees, TokenType};
use fuul_project::FuulProjectClient;
use soroban_sdk::{
    testutils::{Address as TestAddress, Events},
    token::{StellarAssetClient, TokenClient},
    Address, BytesN, Env, Event, String, Vec,
};

use crate::{
    DefaultNativeClaimFeeUpdated, DefaultProjectClaimFeeUpdated, DefaultRemoveFeeUpdated,
    FeeCollectorUpdated, FuulFactory, FuulFactoryClient, NativeClaimFeeUpdated,
    ProjectClaimFeeUpdated, ProjectCreated, RemoveFeeUpdated,
};

#[path = "../../utils/fuul_factory/fixtures.rs"]
mod fixtures;
use fixtures::*;

#[path = "../../utils/fuul_factory/authority.rs"]
mod authority;

#[path = "../../integration/fuul_factory/mod.rs"]
mod integration;

#[path = "../../utils/fuul_factory/creation.rs"]
mod creation_helpers;

#[path = "../../utils/fuul_factory/deployment.rs"]
mod deployment_model;

#[path = "../../utils/fuul_factory/roles.rs"]
mod role_model;

#[path = "../../utils/fuul_factory/fees.rs"]
mod fee_model;

#[path = "../../utils/fuul_factory/retention.rs"]
mod retention_model;

mod administration;
mod constructor;
mod deployment;
mod fees;
mod queries;

#[path = "../../utils/fuul_factory/model.rs"]
mod model;
#[path = "../../fuzz/fuul_factory/mod.rs"]
mod properties;
#[path = "../../utils/fuul_factory/wire.rs"]
mod wire;
