use soroban_sdk::{contractevent, Address, BytesN, Env};
use stellar_contract_utils::upgradeable;
pub use stellar_contract_utils::upgradeable::{Upgradeable, UpgradeableClient};

use crate::{access::require_admin, bump_instance};

/// Emitted after a contract upgrade.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractUpgraded {
    pub operator: Address,
    pub new_wasm_hash: BytesN<32>,
}

/// Upgrades code with authorization from a contract administrator.
pub fn upgrade(e: &Env, new_wasm_hash: BytesN<32>, operator: Address) {
    require_admin(e, &operator);
    upgrade_authorized(e, new_wasm_hash, operator);
}

/// Upgrades code; `operator` must already be authenticated and authorized.
pub fn upgrade_authorized(e: &Env, new_wasm_hash: BytesN<32>, operator: Address) {
    upgradeable::upgrade(e, &new_wasm_hash);
    // Renew the instance and replacement code TTL.
    bump_instance(e);
    ContractUpgraded { operator, new_wasm_hash }.publish(e);
}
