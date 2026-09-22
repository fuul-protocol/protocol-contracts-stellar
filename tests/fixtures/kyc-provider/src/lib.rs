#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Env,
};

#[contracttype]
enum Key {
    Admin,
    Recipient,
    Mode,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FixtureError {
    Unavailable = 9100,
    InvalidMode = 9101,
    UnexpectedRecipient = 9102,
}

#[contract]
pub struct KycFixture;

#[contractimpl]
impl KycFixture {
    pub fn __constructor(e: Env, admin: Address, expected_recipient: Address, mode: u32) {
        if mode > 2 {
            panic_with_error!(&e, FixtureError::InvalidMode);
        }
        e.storage().instance().set(&Key::Admin, &admin);
        e.storage().instance().set(&Key::Recipient, &expected_recipient);
        e.storage().instance().set(&Key::Mode, &mode);
    }

    pub fn set_mode(e: Env, mode: u32) {
        let admin: Address = e.storage().instance().get(&Key::Admin).unwrap();
        admin.require_auth();
        if mode > 2 {
            panic_with_error!(&e, FixtureError::InvalidMode);
        }
        e.storage().instance().set(&Key::Mode, &mode);
    }

    pub fn is_user_kyc_registered(e: Env, user: Address) -> bool {
        let expected: Address = e.storage().instance().get(&Key::Recipient).unwrap();
        if user != expected {
            panic_with_error!(&e, FixtureError::UnexpectedRecipient);
        }
        match e.storage().instance().get::<_, u32>(&Key::Mode).unwrap() {
            0 => false,
            1 => true,
            _ => panic_with_error!(&e, FixtureError::Unavailable),
        }
    }
}
