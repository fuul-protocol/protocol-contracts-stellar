#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, panic_with_error, symbol_short, Address,
    Env,
};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum TokenError {
    InvalidAmount = 1,
    InsufficientBalance = 2,
    Overflow = 3,
}

/// Test-only implementation of Fuul's per-ID transfer interface, not an ERC-1155 contract.
#[contract]
pub struct TestMultiToken;

#[contractevent]
pub struct Transfer {
    #[topic]
    pub from: Address,
    #[topic]
    pub to: Address,
    pub token_id: u32,
    pub amount: i128,
}

fn balance(e: &Env, owner: &Address, token_id: u32) -> i128 {
    e.storage().persistent().get(&(symbol_short!("balance"), owner, token_id)).unwrap_or(0)
}

#[contractimpl]
impl TestMultiToken {
    pub fn __constructor(e: &Env, issuer: Address) {
        e.storage().instance().set(&symbol_short!("issuer"), &issuer);
    }

    pub fn mint(e: &Env, to: Address, token_id: u32, amount: i128) {
        let issuer: Address = e.storage().instance().get(&symbol_short!("issuer")).unwrap();
        issuer.require_auth();
        if amount < 0 {
            panic_with_error!(e, TokenError::InvalidAmount);
        }
        let total = balance(e, &to, token_id)
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(e, TokenError::Overflow));
        e.storage().persistent().set(&(symbol_short!("balance"), &to, token_id), &total);
    }

    pub fn balance(e: &Env, owner: Address, token_id: u32) -> i128 {
        balance(e, &owner, token_id)
    }

    pub fn transfer(e: &Env, from: Address, to: Address, token_id: u32, amount: i128) {
        from.require_auth();
        if amount < 0 {
            panic_with_error!(e, TokenError::InvalidAmount);
        }
        let available = balance(e, &from, token_id);
        if available < amount {
            panic_with_error!(e, TokenError::InsufficientBalance);
        }
        if from != to {
            let total = balance(e, &to, token_id)
                .checked_add(amount)
                .unwrap_or_else(|| panic_with_error!(e, TokenError::Overflow));
            e.storage()
                .persistent()
                .set(&(symbol_short!("balance"), &from, token_id), &(available - amount));
            e.storage().persistent().set(&(symbol_short!("balance"), &to, token_id), &total);
        }
        Transfer { from, to, token_id, amount }.publish(e);
    }
}
