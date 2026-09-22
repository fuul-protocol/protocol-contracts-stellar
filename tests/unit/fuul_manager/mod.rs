extern crate std;

use fuul_core::access::FuulAccessControlClient;
use fuul_core::{ClaimCheck, ClaimReason, ProjectClaimResult, TokenType};
use fuul_factory::{FuulFactory, FuulFactoryClient};
use fuul_project::{FuulProjectClient, ProjectError};
use soroban_sdk::{
    auth::{Context, CustomAccountInterface},
    contract, contracterror, contractimpl, contracttype,
    crypto::Hash,
    panic_with_error, symbol_short,
    testutils::{
        Address as TestAddress, AuthorizedFunction, AuthorizedInvocation, Events, Ledger, MockAuth,
        MockAuthInvoke,
    },
    token::{StellarAssetClient, TokenClient},
    vec,
    xdr::{LedgerKey, ScAddress, ScErrorCode, ScErrorType, SorobanAuthorizationEntry},
    Address, BytesN, Env, Error, Event, IntoVal, MuxedAddress, String, Symbol, Val, Vec, U256,
};
use stellar_tokens::non_fungible::{Base as NonFungibleBase, NonFungibleToken};

use crate::{
    storage, ClaimCooldownUpdated, Claimed, CurrencyTokenLimit, FuulManager, FuulManagerClient,
    KycValidatorUpdated, NoClaimFeeAddressAdded, NoClaimFeeAddressRemoved, Paused,
    RequiredSignersUpdated, TokenLimitAdded, TokenLimitUpdated, Unpaused,
};

#[path = "../../utils/fuul_manager/mocks.rs"]
mod mocks;
use mocks::*;

#[path = "../../utils/fuul_manager/fixtures.rs"]
mod fixtures;
use fixtures::*;

pub(crate) fn u(e: &Env, value: impl TryInto<u128>) -> U256 {
    U256::from_u128(e, value.try_into().ok().expect("nonnegative test value"))
}

mod administration;
mod authorization;
mod claims;
mod constructor;
mod currencies;
mod exemptions;
mod kyc;
mod numeric_domains;
mod pausing;
mod queries;
mod security_helpers;
#[path = "../../utils/fuul_manager/settlement.rs"]
mod source_settlement_helpers;
mod test_doubles;

#[path = "../../integration/fuul_manager/mod.rs"]
mod integration;
#[path = "../../fuzz/fuul_manager/mod.rs"]
mod properties;
#[path = "../../integration/protocol/mod.rs"]
mod protocol;
#[path = "../../utils/fuul_manager/roles_pause.rs"]
mod roles_pause_helpers;

#[path = "../../utils/fuul_manager/constructor.rs"]
mod constructor_helpers;

#[path = "../../utils/fuul_manager/accounting.rs"]
pub(crate) mod accounting_helpers;
#[path = "../../utils/fuul_manager/cooldown_domain.rs"]
pub(crate) mod cooldown_helpers;
