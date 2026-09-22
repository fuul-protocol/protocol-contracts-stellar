include!("../../../../contracts/fuul-manager/src/contract.rs");

#[contractimpl]
impl FuulManager {
    pub fn fixture_schema_version(e: &Env) -> u32 {
        e.storage().instance().get(&Symbol::new(e, "fuul_upgrade_fixture_schema")).unwrap_or(0)
    }

    pub fn migrate_fixture(e: &Env, operator: Address) {
        fuul_core::access::require_admin(e, &operator);
        if Self::fixture_schema_version(e) != 0 {
            soroban_sdk::panic_with_error!(e, soroban_sdk::Error::from_contract_error(6900));
        }
        e.storage().instance().set(&Symbol::new(e, "fuul_upgrade_fixture_schema"), &2_u32);
    }
}
