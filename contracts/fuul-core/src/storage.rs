use soroban_sdk::Env;

pub const DAY_IN_LEDGERS: u32 = 17_280;
pub const INSTANCE_EXTEND_AMOUNT: u32 = 90 * DAY_IN_LEDGERS;
pub const INSTANCE_TTL_THRESHOLD: u32 = 30 * DAY_IN_LEDGERS;

pub fn bump_instance(e: &Env) {
    e.storage().instance().extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_EXTEND_AMOUNT);
}
