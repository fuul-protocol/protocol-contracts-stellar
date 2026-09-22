use crate::test::{constructor_helpers::*, roles_pause_helpers::test_env, *};
use fuul_core::INSTANCE_EXTEND_AMOUNT;
use soroban_sdk::testutils::{storage::Persistent, Deployer};
use stellar_access::access_control::AccessControlStorageKey as OzKey;

#[test]
fn bootstrap_immediately_extends_instance_code_and_currency_limits() {
    let env = test_env();
    let input = Bootstrap::new(&env);
    let id = input.register(&env);
    assert_eq!(env.deployer().get_contract_instance_ttl(&id), INSTANCE_EXTEND_AMOUNT);
    assert_eq!(env.deployer().get_contract_code_ttl(&id), INSTANCE_EXTEND_AMOUNT);
    env.as_contract(&id, || {
        for asset in [&input.accepted, &input.native] {
            assert_eq!(
                env.storage().persistent().get_ttl(&limit_key(&env, asset)),
                INSTANCE_EXTEND_AMOUNT
            );
        }
    });
}

#[test]
fn bootstrap_role_entries_keep_minimum_ttl_while_existing_roles_is_renewed() {
    let env = test_env();
    let input = Bootstrap::new(&env);
    let minimum = env.ledger().get().min_persistent_entry_ttl - 1;
    let id = input.register(&env);
    // No public membership/enumeration getter is called before these raw TTL reads.
    env.as_contract(&id, || {
        for (name, account) in [
            ("default_admin", &input.admin),
            ("pauser", &input.pauser),
            ("unpauser", &input.unpauser),
            ("claim_signer", &input.signers.get(0).unwrap()),
        ] {
            let role = Symbol::new(&env, name);
            assert_eq!(
                env.storage().persistent().get_ttl(&OzKey::HasRole(account.clone(), role.clone())),
                minimum
            );
            assert_eq!(
                env.storage().persistent().get_ttl(&OzKey::RoleAccountsCount(role.clone())),
                minimum
            );
            assert_eq!(
                env.storage().persistent().get_ttl(&role_member_key(&env, &role, 0)),
                minimum
            );
        }
        // Adding later role kinds reads and renews the already-created enumeration key.
        assert_eq!(
            env.storage().persistent().get_ttl(&OzKey::ExistingRoles),
            INSTANCE_EXTEND_AMOUNT
        );
    });
}
