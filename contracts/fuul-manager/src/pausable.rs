use soroban_sdk::{contracttype, Address, Env};
use stellar_contract_utils::pausable::{when_not_paused, when_paused};

use crate::events::{Paused, Unpaused};

// Shares the OpenZeppelin storage key while emitting actor-aware events.
#[contracttype]
enum PausableStorageKey {
    Paused,
}

pub(crate) fn pause(e: &Env, account: Address) {
    when_not_paused(e);
    e.storage().instance().set(&PausableStorageKey::Paused, &true);
    Paused { account }.publish(e);
}

pub(crate) fn unpause(e: &Env, account: Address) {
    when_paused(e);
    e.storage().instance().set(&PausableStorageKey::Paused, &false);
    Unpaused { account }.publish(e);
}
