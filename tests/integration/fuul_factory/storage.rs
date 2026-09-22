use crate::test::{authority::*, creation_helpers::register, wire::*, *};
use fuul_core::{INSTANCE_EXTEND_AMOUNT as EXTEND, INSTANCE_TTL_THRESHOLD as THRESHOLD};
use soroban_sdk::{
    testutils::{storage::Persistent, Deployer, Ledger},
    xdr::{LedgerKey, ScVal},
    IntoVal, Symbol, TryFromVal, Val,
};

fn setup(compiled: bool) -> (Env, Address, Address, Address, Address, BytesN<32>) {
    let e = test_env();
    e.ledger().with_mut(|l| l.min_persistent_entry_ttl = EXTEND + 1);
    let admin = Address::generate(&e);
    let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
    let id = register(&e, compiled, &code, &admin);
    let first = Address::generate(&e);
    let second = Address::generate(&e);
    e.mock_all_auths();
    let c = FuulFactoryClient::new(&e, &id);
    c.set_native_user_claim_fee(&admin, &first, &7);
    c.set_native_user_claim_fee(&admin, &second, &9);
    (e, id, admin, first, second, code)
}

fn persistent_ttl(e: &Env, id: &Address, key: &Val) -> u32 {
    e.as_contract(id, || e.storage().persistent().get_ttl(key))
}

fn code_deadline(e: &Env, code: &BytesN<32>) -> u32 {
    e.to_ledger_snapshot()
        .ledger_entries
        .into_iter()
        .find_map(|(key, (_, ttl))| {
            matches!(key.as_ref(), LedgerKey::ContractCode(k) if k.hash.0 == code.to_array())
                .then_some(ttl.unwrap())
        })
        .unwrap()
}

#[test]
fn native_and_guest_getters_and_maintenance_obey_thresholds_without_renewing_unrelated_entries() {
    for compiled in [false, true] {
        for ttl in [THRESHOLD - 1, THRESHOLD, THRESHOLD + 1] {
            for name in [
                "keep_alive",
                "project_wasm_hash",
                "contract_tracker",
                "fee_collector",
                "default_native_claim_fee",
                "default_project_claim_fee",
                "default_remove_fee",
                "project_fees",
                "get_fees_information",
            ] {
                let (e, id, admin, first, second, code) = setup(compiled);
                let current = e.deployer().get_contract_instance_ttl(&id);
                e.ledger().with_mut(|l| l.sequence_number += current - ttl);
                let role_key: Val = (Symbol::new(&e, "HasRole"), &admin, role(&e)).into_val(&e);
                let role_before = persistent_ttl(&e, &id, &role_key);
                let first_before = persistent_ttl(&e, &id, &fee_key(&e, &first));
                let second_before = persistent_ttl(&e, &id, &fee_key(&e, &second));
                let future_code = code_deadline(&e, &code);
                let before = state(&e, &id);
                let accesses_fees = matches!(name, "project_fees" | "get_fees_information");
                let args = if accesses_fees { (&first,).into_val(&e) } else { Vec::new(&e) };
                let _: Val = e.invoke_contract(&id, &Symbol::new(&e, name), args);
                assert!(e.events().all().events().is_empty());
                let expected = if ttl <= THRESHOLD { EXTEND } else { ttl };
                let expected_instance = if name == "project_fees" { ttl } else { expected };
                assert_eq!(
                    e.deployer().get_contract_instance_ttl(&id),
                    expected_instance,
                    "{name}: instance"
                );
                assert_eq!(
                    e.deployer().get_contract_code_ttl(&id),
                    expected_instance,
                    "{name}: code"
                );
                assert_eq!(code_deadline(&e, &code), future_code, "{name}: future Project code");
                assert_eq!(persistent_ttl(&e, &id, &role_key), role_before);
                assert_eq!(persistent_ttl(&e, &id, &fee_key(&e, &second)), second_before);
                assert_eq!(
                    persistent_ttl(&e, &id, &fee_key(&e, &first)),
                    if accesses_fees { expected } else { first_before }
                );
                let after = state(&e, &id);
                let selected = ScVal::try_from_val(&e, &fee_key(&e, &first)).unwrap();
                for ((key, _, old_ttl), (_, _, new_ttl)) in before.iter().zip(&after) {
                    if let LedgerKey::ContractData(data) = key {
                        if data.key != ScVal::LedgerKeyContractInstance
                            && !(accesses_fees && data.key == selected)
                        {
                            assert_eq!(new_ttl, old_ttl, "{name}: unrelated persistent key");
                        }
                    }
                }
                assert_eq!(
                    before.iter().map(|(k, value, _)| (k, value)).collect::<std::vec::Vec<_>>(),
                    after.iter().map(|(k, value, _)| (k, value)).collect::<std::vec::Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn present_zero_fees_renew_but_absent_queries_do_not_create_entries() {
    for ttl in [THRESHOLD - 1, THRESHOLD, THRESHOLD + 1] {
        let (e, id, admin, first, _, _) = setup(true);
        FuulFactoryClient::new(&e, &id).set_native_user_claim_fee(&admin, &first, &0);
        let current = e.deployer().get_contract_instance_ttl(&id);
        e.ledger().with_mut(|l| l.sequence_number += current - ttl);
        let absent = Address::generate(&e);
        assert_eq!(result(&e, &id, "project_fees", (&absent,).into_val(&e)), fees([0, 0, 0]));
        e.as_contract(&id, || assert!(!e.storage().persistent().has(&fee_key(&e, &absent))));
        assert_eq!(result(&e, &id, "project_fees", (&first,).into_val(&e)), fees([0, 0, 0]));
        assert_eq!(
            persistent_ttl(&e, &id, &fee_key(&e, &first)),
            if ttl <= THRESHOLD { EXTEND } else { ttl }
        );
        assert_eq!(e.deployer().get_contract_instance_ttl(&id), ttl);
        assert_eq!(e.deployer().get_contract_code_ttl(&id), ttl);
    }
}

#[test]
fn expired_fee_entry_is_restored_by_sdk_without_becoming_an_absent_mapping() {
    let (e, id, _, first, _, _) = setup(true);
    e.ledger().with_mut(|l| l.sequence_number += EXTEND - 1);
    FuulFactoryClient::new(&e, &id).keep_alive();
    e.ledger().with_mut(|l| l.sequence_number += 2);
    let raw_key = ScVal::Vec(Some(soroban_sdk::xdr::ScVec(
        std::vec![symbol("ProjectFees"), address(&first)].try_into().unwrap(),
    )));
    let expired = state(&e, &id)
        .into_iter()
        .find(|(key, _, _)| matches!(key, LedgerKey::ContractData(k) if k.key == raw_key))
        .unwrap();
    assert!(expired.2.unwrap() < e.ledger().sequence());
    assert_eq!(result(&e, &id, "project_fees", (&first,).into_val(&e)), fees([7, 0, 0]));
    assert!(persistent_ttl(&e, &id, &fee_key(&e, &first)) > 0);
    let absent = Address::generate(&e);
    assert_eq!(result(&e, &id, "project_fees", (&absent,).into_val(&e)), fees([0, 0, 0]));
}
