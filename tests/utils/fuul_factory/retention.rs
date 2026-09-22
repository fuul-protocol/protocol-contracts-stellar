use crate::test::{authority::*, creation_helpers::register, wire::*, *};
use fuul_core::{INSTANCE_EXTEND_AMOUNT as EXTEND, INSTANCE_TTL_THRESHOLD as THRESHOLD};
use soroban_sdk::{
    testutils::{storage::Persistent, Deployer, Ledger},
    xdr::{LedgerKey, ScErrorCode, ScErrorType, ScVal},
    Error, IntoVal, Symbol, Val,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct Case {
    pub delta: i8,
    pub query: u8,
    pub values: [i128; 3],
    pub absent: bool,
    pub archive: bool,
    pub extra: u32,
}

fn ttl(e: &Env, id: &Address, key: &Val) -> u32 {
    e.as_contract(id, || e.storage().persistent().get_ttl(key))
}

pub(super) fn exercise(case: Case, guest: bool) -> u64 {
    let e = test_env();
    e.cost_estimate().budget().reset_unlimited();
    e.ledger().with_mut(|ledger| ledger.min_persistent_entry_ttl = EXTEND + 1);
    let admin = Address::generate(&e);
    let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
    let id = register(&e, guest, &code, &admin);
    let c = FuulFactoryClient::new(&e, &id);
    let first = Address::generate(&e);
    let second = Address::generate(&e);
    let absent = Address::generate(&e);
    e.mock_all_auths();
    // Create a present record even when every requested fee is zero.
    c.set_native_user_claim_fee(&admin, &first, &1);
    if case.values[0] != 1 {
        c.set_native_user_claim_fee(&admin, &first, &case.values[0]);
    }
    if case.values[1] != 0 {
        c.set_project_claim_fee(&admin, &first, &(case.values[1] as u32));
    }
    if case.values[2] != 0 {
        c.set_remove_fee(&admin, &first, &(case.values[2] as u32));
    }
    c.set_native_user_claim_fee(&admin, &second, &7);
    e.mock_auths(&[]);
    let desired = (i64::from(THRESHOLD) + i64::from(case.delta)) as u32;
    let current = e.deployer().get_contract_instance_ttl(&id);
    e.ledger().with_mut(|ledger| ledger.sequence_number += current - desired);
    // Failure rollback is measured while all touched entries are still live.
    for authenticated in [true, false] {
        let args = (&admin, &first, case.values[0]).into_val(&e);
        if authenticated {
            authorize(&e, &id, &admin, "set_native_user_claim_fee", args);
        } else {
            e.mock_auths(&[]);
        }
        let before = e.to_ledger_snapshot().ledger_entries;
        let expected = if authenticated {
            Error::from_contract_error(6201)
        } else {
            Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)
        };
        assert_eq!(
            c.try_set_native_user_claim_fee(&admin, &first, &case.values[0]),
            Err(Ok(expected))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
    }
    let first_key = fee_key(&e, &first);
    let second_key = fee_key(&e, &second);
    let first_ttl = ttl(&e, &id, &first_key);
    let second_ttl = ttl(&e, &id, &second_key);
    let role_key: Val = (Symbol::new(&e, "HasRole"), &admin, role(&e)).into_val(&e);
    let role_ttl = ttl(&e, &id, &role_key);
    let before = state(&e, &id);
    let future_code = e.to_ledger_snapshot().ledger_entries.into_iter().find(|(key, _)| matches!(key.as_ref(), LedgerKey::ContractCode(k) if k.hash.0 == code.to_array())).unwrap();
    let names = [
        "project_fees",
        "get_fees_information",
        "keep_alive",
        "default_native_claim_fee",
        "default_project_claim_fee",
        "default_remove_fee",
        "project_wasm_hash",
        "contract_tracker",
        "fee_collector",
    ];
    let selected = if case.absent { &absent } else { &first };
    let args = if case.query < 2 { (selected,).into_val(&e) } else { Vec::new(&e) };
    let actual = result(&e, &id, names[usize::from(case.query)], args);
    let selected_fees = if case.absent { [0; 3] } else { case.values };
    let expected = match case.query {
        0 => fees(selected_fees),
        1 => map(&[("fee_collector", address(&admin)), ("fees", fees(selected_fees))]),
        2 => ScVal::Void,
        3 => signed(20_000),
        4 => ScVal::U32(100),
        5 => ScVal::U32(0),
        6 => ScVal::Bytes(soroban_sdk::xdr::ScBytes(code.to_array().to_vec().try_into().unwrap())),
        7 => unsigned(0),
        8 => address(&admin),
        _ => unreachable!(),
    };
    assert_eq!(actual, expected);
    assert!(e.events().all().events().is_empty());
    let bumped = if desired <= THRESHOLD { EXTEND } else { desired };
    let instance = if case.query == 0 { desired } else { bumped };
    assert_eq!(e.deployer().get_contract_instance_ttl(&id), instance);
    assert_eq!(e.deployer().get_contract_code_ttl(&id), instance);
    assert_eq!(
        ttl(&e, &id, &first_key),
        if case.query < 2 && !case.absent && first_ttl <= THRESHOLD { EXTEND } else { first_ttl }
    );
    assert_eq!(ttl(&e, &id, &second_key), second_ttl);
    assert_eq!(ttl(&e, &id, &role_key), role_ttl);
    e.as_contract(&id, || assert!(!e.storage().persistent().has(&fee_key(&e, &absent))));
    assert!(e.to_ledger_snapshot().ledger_entries.contains(&future_code));
    let after = state(&e, &id);
    assert_eq!(
        before.iter().map(|(k, v, _)| (k, v)).collect::<std::vec::Vec<_>>(),
        after.iter().map(|(k, v, _)| (k, v)).collect::<std::vec::Vec<_>>()
    );
    for ((key, _, old), (_, _, new)) in before.iter().zip(&after) {
        if let LedgerKey::ContractData(data) = key {
            let selected = soroban_sdk::xdr::ScVal::Vec(Some(soroban_sdk::xdr::ScVec(
                std::vec![symbol("ProjectFees"), address(&first)].try_into().unwrap(),
            )));
            if data.key != ScVal::LedgerKeyContractInstance
                && !(data.key == selected && case.query < 2 && !case.absent)
            {
                assert_eq!(old, new);
            }
        }
    }
    if case.archive {
        c.keep_alive();
        let remaining = ttl(&e, &id, &first_key);
        e.ledger().with_mut(|ledger| ledger.sequence_number += remaining - 1);
        c.keep_alive();
        e.ledger().with_mut(|ledger| ledger.sequence_number += case.extra + 2);
        let selected = ScVal::Vec(Some(soroban_sdk::xdr::ScVec(
            std::vec![symbol("ProjectFees"), address(&first)].try_into().unwrap(),
        )));
        let expired = state(&e, &id)
            .into_iter()
            .find(
                |(key, _, _)| matches!(key, LedgerKey::ContractData(data) if data.key == selected),
            )
            .unwrap();
        assert!(expired.2.unwrap() < e.ledger().sequence());
        // SDK restoration is a separate successful read, not part of failed-call rollback.
        assert_eq!(result(&e, &id, "project_fees", (&first,).into_val(&e)), fees(case.values));
        assert!(ttl(&e, &id, &first_key) > 0);
        assert_eq!(result(&e, &id, "project_fees", (&absent,).into_val(&e)), fees([0; 3]));
        e.as_contract(&id, || assert!(!e.storage().persistent().has(&fee_key(&e, &absent))));
        assert!(e.events().all().events().is_empty());
    }
    (1 << case.query)
        | (1 << (10 + case.delta))
        | if case.archive { 1 << 12 } else { 0 }
        | if case.absent { 1 << 13 } else { 0 }
        | if case.values == [0; 3] { 1 << 14 } else { 0 }
        | if case.values[0] > i128::from(u64::MAX) { 1 << 15 } else { 0 }
}
