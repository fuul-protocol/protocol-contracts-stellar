use crate::test::{roles_pause_helpers::*, *};
use fuul_core::{INSTANCE_EXTEND_AMOUNT, INSTANCE_TTL_THRESHOLD};

fn renews_instance(operation: &str) {
    let env = test_env();
    let f = fixture(&env);
    // Keep the operational role entries live while aging the instance independently.
    let access = FuulAccessControlClient::new(&env, &f.client.address);
    assert!(access.has_role(&f.client.pauser_role(), &f.pauser));
    assert!(access.has_role(&f.client.unpauser_role(), &f.unpauser));
    if operation == "unpause" {
        transition(&env, &f.client, &f.pauser, true);
    }
    let remaining = ttl(&env, &f.client);
    assert!(remaining > INSTANCE_TTL_THRESHOLD);
    env.ledger()
        .with_mut(|ledger| ledger.sequence_number += remaining - INSTANCE_TTL_THRESHOLD + 1);
    assert_eq!(ttl(&env, &f.client), INSTANCE_TTL_THRESHOLD - 1);
    match operation {
        "pause" => transition(&env, &f.client, &f.pauser, true),
        "unpause" => transition(&env, &f.client, &f.unpauser, false),
        _ => assert!(!f.client.paused()),
    }
    // Capture events before as_contract starts a new invocation for the TTL read.
    let events = env.events().all();
    assert_eq!(ttl(&env, &f.client), INSTANCE_EXTEND_AMOUNT);
    match operation {
        "pause" => assert_eq!(
            events,
            std::vec![Paused { account: f.pauser.clone() }.to_xdr(&env, &f.client.address)]
        ),
        "unpause" => assert_eq!(
            events,
            std::vec![Unpaused { account: f.unpauser.clone() }.to_xdr(&env, &f.client.address)]
        ),
        _ => assert!(events.events().is_empty()),
    }
    assert_eq!(f.client.paused(), operation == "pause");
}

#[test]
fn pause_renews_aged_instance_ttl() {
    renews_instance("pause");
}
#[test]
fn unpause_renews_aged_instance_ttl() {
    renews_instance("unpause");
}
#[test]
fn paused_query_renews_aged_instance_ttl() {
    renews_instance("paused");
}
