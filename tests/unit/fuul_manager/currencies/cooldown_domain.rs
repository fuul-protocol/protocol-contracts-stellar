use crate::test::cooldown_helpers::scenario;

#[test]
fn cooldown_sum_at_and_above_u64_max_preserves_before_equal_and_limit_semantics() {
    for (start, now, cooldown) in [
        (1, u64::MAX - 1, u64::MAX - 1),
        (1, u64::MAX, u64::MAX - 1),
        (1, u64::MAX, u64::MAX),
        (u64::MAX - 86_400, u64::MAX - 1, 86_400),
        (u64::MAX - 86_400, u64::MAX, 86_400),
        (u64::MAX, u64::MAX, u64::MAX),
        (0, u64::MAX, u64::MAX),
    ] {
        scenario(start, now, cooldown, false);
    }
}
