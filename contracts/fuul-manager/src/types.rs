use soroban_sdk::{contracttype, Env, U256};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CurrencyTokenLimit {
    pub claim_limit_per_cooldown: U256,
    pub cumulative_claim_per_cooldown: U256,
    pub claim_cooldown_period_started: u64,
}

impl CurrencyTokenLimit {
    pub fn zero(e: &Env) -> Self {
        Self {
            claim_limit_per_cooldown: U256::from_u32(e, 0),
            cumulative_claim_per_cooldown: U256::from_u32(e, 0),
            claim_cooldown_period_started: 0,
        }
    }
}
