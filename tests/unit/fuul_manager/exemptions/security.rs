use crate::test::{roles_pause_helpers as auth, *};
use fuul_core::{INSTANCE_EXTEND_AMOUNT as EXTEND, INSTANCE_TTL_THRESHOLD as THRESHOLD};
use soroban_sdk::testutils::storage::Persistent;

#[test]
fn exempt_recipient_does_not_exempt_the_distinct_authorized_fee_payer() {
    let env = auth::test_env();
    let f = claim_fixture(&env);
    assert_ne!(f.caller, f.recipient);
    auth::authorize(
        &env,
        &f.client.address,
        &f.admin,
        "add_no_claim_fee_address",
        (&f.admin, &f.recipient).into_val(&env),
    );
    f.client.add_no_claim_fee_address(&f.admin, &f.recipient);
    let collector = Address::generate(&env);
    let project = register_mock_project(&env, &collector, 20_000);
    let check = claim_check(&env, &f, &project.address, 10, 77);
    let checks = vec![&env, check.clone()];
    let payload = check.authorization();
    env.mock_auths(&[
        MockAuth {
            address: &f.caller,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "claim",
                args: (&f.caller, &checks).into_val(&env),
                sub_invokes: &[MockAuthInvoke {
                    contract: &f.native_asset,
                    fn_name: "transfer",
                    args: (&f.caller, &collector, 20_000_i128).into_val(&env),
                    sub_invokes: &[],
                }],
            },
        },
        MockAuth {
            address: &f.signer,
            invoke: &MockAuthInvoke {
                contract: &f.client.address,
                fn_name: "claim",
                args: (payload,).into_val(&env),
                sub_invokes: &[],
            },
        },
    ]);
    f.client.claim(&f.caller, &checks);
    let native = TokenClient::new(&env, &f.native_asset);
    assert_eq!(native.balance(&f.caller), 980_000);
    assert_eq!(native.balance(&collector), 20_000);
    assert_eq!(native.balance(&f.recipient), 0);
    assert!(f.client.no_claim_fee_addresses(&f.recipient));
    assert!(!f.client.no_claim_fee_addresses(&f.caller));
    assert_eq!(f.client.users_claims(&f.recipient, &f.currency), u(&env, 10));
}

#[test]
fn exemption_remove_physically_deletes_and_readd_renews_only_that_account() {
    let env = auth::test_env();
    env.ledger().with_mut(|l| l.min_persistent_entry_ttl = EXTEND + 1);
    let f = fixture(&env);
    let account = Address::generate(&env);
    let other = Address::generate(&env);
    let key: Val = (Symbol::new(&env, "FeeExemption"), &account).into_val(&env);
    let other_key: Val = (Symbol::new(&env, "FeeExemption"), &other).into_val(&env);
    env.mock_all_auths();
    f.client.add_no_claim_fee_address(&f.admin, &account);
    f.client.add_no_claim_fee_address(&f.admin, &other);
    env.ledger().with_mut(|l| l.sequence_number += EXTEND - THRESHOLD + 1);
    env.as_contract(&f.client.address, || {
        assert_eq!(env.storage().persistent().get_ttl(&key), THRESHOLD - 1);
        assert_eq!(env.storage().persistent().get_ttl(&other_key), THRESHOLD - 1);
    });
    auth::authorize(
        &env,
        &f.client.address,
        &f.admin,
        "remove_no_claim_fee_address",
        (&f.admin, &account).into_val(&env),
    );
    f.client.remove_no_claim_fee_address(&f.admin, &account);
    assert_eq!(
        env.events().all(),
        std::vec![
            NoClaimFeeAddressRemoved { account: account.clone() }.to_xdr(&env, &f.client.address)
        ]
    );
    env.as_contract(&f.client.address, || {
        assert!(!env.storage().persistent().has(&key));
        assert_eq!(env.storage().persistent().get_ttl(&other_key), THRESHOLD - 1);
    });
    assert!(!f.client.no_claim_fee_addresses(&account));
    auth::authorize(
        &env,
        &f.client.address,
        &f.admin,
        "add_no_claim_fee_address",
        (&f.admin, &account).into_val(&env),
    );
    f.client.add_no_claim_fee_address(&f.admin, &account);
    assert_eq!(
        env.events().all(),
        std::vec![
            NoClaimFeeAddressAdded { account: account.clone() }.to_xdr(&env, &f.client.address)
        ]
    );
    env.as_contract(&f.client.address, || {
        assert_eq!(env.storage().persistent().get::<_, bool>(&key), Some(true));
        assert_eq!(env.storage().persistent().get_ttl(&key), EXTEND);
        assert_eq!(env.storage().persistent().get_ttl(&other_key), THRESHOLD - 1);
    });
}
