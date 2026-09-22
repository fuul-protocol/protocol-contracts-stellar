#![no_std]
use soroban_sdk::{contract, contractimpl};

#[contract]
pub struct Replacement;

#[contractimpl]
impl Replacement {
    pub fn revision() -> u32 {
        2
    }
}
