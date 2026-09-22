#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String};
use stellar_tokens::non_fungible::{Base, NonFungibleToken};

/// Test-only issuer-controlled collection using the pinned OpenZeppelin NFT implementation.
#[contract]
pub struct TestNft;

#[contractimpl]
impl TestNft {
    pub fn __constructor(e: &Env, issuer: Address) {
        e.storage().instance().set(&symbol_short!("issuer"), &issuer);
    }

    pub fn mint(e: &Env, to: Address, token_id: u32) {
        let issuer: Address = e.storage().instance().get(&symbol_short!("issuer")).unwrap();
        issuer.require_auth();
        Base::mint(e, &to, token_id);
    }
}

#[contractimpl(contracttrait)]
impl NonFungibleToken for TestNft {
    type ContractType = Base;
}
