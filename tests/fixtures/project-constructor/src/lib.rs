#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, panic_with_error, symbol_short, Address,
    Env, String, Vec,
};

#[contractevent]
pub struct Started {
    pub admin: Address,
}

#[contracterror]
#[derive(Copy, Clone)]
#[repr(u32)]
pub enum ProbeError {
    NotReady = 8200,
}

#[contract]
pub struct ConstructorProbe;

#[contractimpl]
impl ConstructorProbe {
    // Test-only: the admin argument is a repairable gate, not a production role.
    pub fn __constructor(e: Env, factory: Address, admin: Address, uri: String, kyc: bool) {
        factory.require_auth();
        e.storage().instance().set(&symbol_short!("probe"), &(factory, uri, kyc));
        Started { admin: admin.clone() }.publish(&e);
        if !e.invoke_contract::<bool>(&admin, &symbol_short!("ready"), Vec::new(&e)) {
            panic_with_error!(&e, ProbeError::NotReady);
        }
    }
}
