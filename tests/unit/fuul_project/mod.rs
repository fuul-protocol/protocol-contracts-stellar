extern crate std;

use fuul_core::access::FuulAccessControlClient;
use fuul_core::{
    FeesInformation, ProjectClaimResult, ProjectFees, TokenType, INSTANCE_EXTEND_AMOUNT,
};
use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, symbol_short,
    testutils::{Address as TestAddress, AuthorizedFunction, AuthorizedInvocation, Events, Ledger},
    token::{StellarAssetClient, TokenClient},
    vec, Address, BytesN, Env, Event, IntoVal, MuxedAddress, String, Symbol, Vec, U256,
};
use stellar_tokens::non_fungible::{Base as NonFungibleBase, NonFungibleToken};

use crate::{
    events::{FundsRemoved, KycRequiredUpdated, ProjectInfoUpdated},
    FuulProject, FuulProjectClient,
};

#[path = "../../utils/fuul_project/mocks.rs"]
mod mocks;
use mocks::*;

#[path = "../../utils/fuul_project/fixtures.rs"]
mod fixtures;
use fixtures::*;

#[path = "../../utils/fuul_project/guest.rs"]
mod guest;

#[path = "../../utils/fuul_project/economics.rs"]
mod economics;

#[path = "../../utils/fuul_project/authorization.rs"]
mod authorization_cases;

#[path = "../../utils/fuul_project/batches.rs"]
mod batch_cases;

#[path = "../../utils/fuul_project/retention.rs"]
mod retention_cases;

#[path = "../../integration/fuul_project/mod.rs"]
mod integration;
#[path = "../../utils/fuul_project/model.rs"]
mod model;
#[path = "../../fuzz/fuul_project/mod.rs"]
mod properties;

fn u(e: &Env, value: impl TryInto<u128>) -> U256 {
    U256::from_u128(e, value.try_into().ok().expect("nonnegative test value"))
}

mod administration;
mod assets;
mod claims;
mod configuration;
mod constructor;
mod fees;
mod queries;
mod removal;
