#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, MuxedAddress};

// Test-only implementation of the SEP-41 operations consumed by Manager/Project.
#[contract]
pub struct FungibleFixture;

#[contractimpl]
impl FungibleFixture {
    pub fn __constructor(e: Env, admin: Address, decimals: u32) {
        e.storage().instance().set(&symbol_short!("admin"), &admin);
        e.storage().instance().set(&symbol_short!("decimals"), &decimals);
    }

    pub fn decimals(e: Env) -> u32 {
        e.storage().instance().get(&symbol_short!("decimals")).unwrap()
    }

    pub fn balance(e: Env, id: Address) -> i128 {
        e.storage().persistent().get(&id).unwrap_or(0)
    }

    pub fn mint(e: Env, to: Address, amount: i128) {
        let admin: Address = e.storage().instance().get(&symbol_short!("admin")).unwrap();
        admin.require_auth();
        assert!(amount >= 0);
        let balance = Self::balance(e.clone(), to.clone()).checked_add(amount).unwrap();
        e.storage().persistent().set(&to, &balance);
    }

    pub fn transfer(e: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let to = to.address();
        let balance = Self::balance(e.clone(), from.clone());
        assert!(amount >= 0 && amount <= balance);
        if from == to {
            return;
        }
        let received = Self::balance(e.clone(), to.clone()).checked_add(amount).unwrap();
        e.storage().persistent().set(&from, &(balance - amount));
        e.storage().persistent().set(&to, &received);
    }
}
