use crate::test::model::{exercise, Action::*};

#[test]
fn fixed_sequence_preserves_snapshots_live_collector_and_rejected_state() {
    let sequence = [
        Read(0),
        Override(0, 0, 1),
        Create(true),
        Default(0, 7),
        Default(1, 10_000),
        Default(2, 100),
        Create(true),
        Override(2, 2, 1),
        Collector(1),
        Default(1, 10_001),
        Override(3, 1, 10_000),
        Create(false),
        Denied,
        Read(2),
        Collector(1),
        Create(true),
    ];
    for compiled in [false, true] {
        exercise(&sequence, compiled);
    }
}
