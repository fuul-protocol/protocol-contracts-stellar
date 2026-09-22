extern crate std;

use crate::test::cooldown_helpers::scenario;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]
    #[test]
    fn full_u64_clock_and_cooldown_domain_matches_elapsed_time_model(
        first in any::<u64>(), second in any::<u64>(), raw_period in any::<u64>()
    ) {
        let start = first.min(second);
        let now = first.max(second);
        // Only the API's existing one-day minimum restricts generated periods.
        scenario(start, now, raw_period.max(86_400), false);
    }
}
