extern crate std;

use crate::FuulManagerClient;
use fuul_core::{access::FuulAccessControlClient, ClaimCheck, ClaimReason, TokenType};
use fuul_factory::FuulFactoryClient;
use fuul_project::FuulProjectClient;
use soroban_sdk::{
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Events, Ledger, MockAuth, MockAuthInvoke},
    token::{StellarAssetClient, TokenClient},
    vec, Address, BytesN, Env, Error, Event, IntoVal, Map, String, Symbol, Val, Vec, U256,
};

mod administration;
mod assets;
mod authorization;
mod lifecycle;
mod multicurrency;
mod settlement;

const MANAGER: &[u8] = include_bytes!("../../target/wasm32v1-none/release/fuul_manager.wasm");
const FACTORY: &[u8] = include_bytes!("../../target/wasm32v1-none/release/fuul_factory.wasm");
const PROJECT: &[u8] = include_bytes!("../../target/wasm32v1-none/release/fuul_project.wasm");
const REPLACEMENT: &[u8] =
    include_bytes!("../../target/wasm32v1-none/release/fuul_upgrade_test.wasm");

fn u(e: &Env, value: u128) -> U256 {
    U256::from_u128(e, value)
}
fn error(code: u32) -> Error {
    Error::from_contract_error(code)
}

struct Fixture<'a> {
    e: &'a Env,
    manager: FuulManagerClient<'a>,
    factory: FuulFactoryClient<'a>,
    project: FuulProjectClient<'a>,
    admin: Address,
    factory_admin: Address,
    project_admin: Address,
    pauser: Address,
    unpauser: Address,
    signer: Address,
    caller: Address,
    recipient: Address,
    collector: Address,
    currency: Address,
    native: Address,
}

impl<'a> Fixture<'a> {
    fn new(e: &'a Env) -> Self {
        e.ledger().with_mut(|l| {
            l.timestamp = 1_000_000;
            l.sequence_number = 100;
        });
        e.mock_all_auths();
        let admin = Address::generate(e);
        let factory_admin = Address::generate(e);
        let project_admin = Address::generate(e);
        let pauser = Address::generate(e);
        let unpauser = Address::generate(e);
        let signer = Address::generate(e);
        let caller = Address::generate(e);
        let recipient = Address::generate(e);
        let collector = Address::generate(e);
        let currency = e.register_stellar_asset_contract_v2(admin.clone()).address();
        let native = e.register_stellar_asset_contract_v2(admin.clone()).address();
        let manager = FuulManagerClient::new(
            e,
            &e.register(
                MANAGER,
                (
                    &admin,
                    &pauser,
                    &unpauser,
                    1_u128,
                    vec![e, signer.clone()],
                    &currency,
                    &native,
                    None::<Address>,
                    u(e, 1_000_000),
                ),
            ),
        );
        let hash = e.deployer().upload_contract_wasm(PROJECT);
        let factory = FuulFactoryClient::new(
            e,
            &e.register(FACTORY, (&factory_admin, &manager.address, &collector, &hash)),
        );
        let project = FuulProjectClient::new(
            e,
            &factory.create_fuul_project(
                &project_admin,
                &String::from_str(e, "ipfs://project"),
                &false,
            ),
        );
        StellarAssetClient::new(e, &currency).mint(&project.address, &1_000_000);
        StellarAssetClient::new(e, &native).mint(&caller, &1_000_000);
        Self {
            e,
            manager,
            factory,
            project,
            admin,
            factory_admin,
            project_admin,
            pauser,
            unpauser,
            signer,
            caller,
            recipient,
            collector,
            currency,
            native,
        }
    }
    fn check(&self, proof: u8, amount: i128) -> ClaimCheck {
        ClaimCheck {
            project_address: self.project.address.clone(),
            to: self.recipient.clone(),
            currency: self.currency.clone(),
            currency_type: TokenType::StellarAsset,
            amount,
            reason: ClaimReason::AffiliatePayout,
            token_id: u(self.e, 0),
            deadline: u(self.e, self.e.ledger().timestamp() as u128 + 300),
            proof: BytesN::from_array(self.e, &[proof; 32]),
            signers: vec![self.e, self.signer.clone()],
        }
    }
    fn claim(&self, check: &ClaimCheck) {
        self.manager.claim(&self.caller, &vec![self.e, check.clone()]);
    }
    fn balances(&self) -> [i128; 5] {
        let token = TokenClient::new(self.e, &self.currency);
        let native = TokenClient::new(self.e, &self.native);
        [
            token.balance(&self.project.address),
            token.balance(&self.recipient),
            token.balance(&self.collector),
            native.balance(&self.caller),
            native.balance(&self.collector),
        ]
    }
    fn assert_unsettled(&self, check: &ClaimCheck) {
        assert!(!self.project.claimed_proofs(&check.proof));
        assert_eq!(self.balances(), [1_000_000, 0, 0, 1_000_000, 0]);
        assert_eq!(self.manager.users_claims(&self.recipient, &self.currency), u(self.e, 0));
        assert_eq!(
            self.manager.currency_limits(&self.currency).cumulative_claim_per_cooldown,
            u(self.e, 0)
        );
    }
}
