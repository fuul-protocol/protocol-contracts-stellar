use crate::test::{creation_helpers::*, *};

#[test]
fn native_counter_does_not_stop_at_u64_limit() {
    let e = authority::test_env();
    let f = fixture(&e);
    assert_crosses_u64(&e, &f.client.address, &f.project_admin);
}

#[test]
fn native_tracker_uses_uint96_domain_and_wraps_after_current_salt() {
    let e = authority::test_env();
    let f = fixture(&e);
    assert_domain(&e, &f.client.address, &f.project_admin);
}
