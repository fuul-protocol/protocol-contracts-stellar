#![no_std]

use soroban_sdk::{contract, contractimpl, symbol_short, Env};

#[contract]
pub struct ConstructorGate;

#[contractimpl]
impl ConstructorGate {
    pub fn ready(e: Env) -> bool {
        e.storage().instance().get(&symbol_short!("ready")).unwrap_or(false)
    }

    pub fn set_ready(e: Env, ready: bool) {
        e.storage().instance().set(&symbol_short!("ready"), &ready);
    }
}
