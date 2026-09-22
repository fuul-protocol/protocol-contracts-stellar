use crate::test::cooldown_helpers::scenario;

#[test]
fn wasm_manager_claim_handles_extreme_cooldown_and_maximum_timestamp() {
    scenario(1, u64::MAX, u64::MAX, true);
    scenario(1, u64::MAX, u64::MAX - 1, true);
}
