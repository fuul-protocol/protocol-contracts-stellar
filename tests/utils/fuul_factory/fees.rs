use crate::test::{authority::*, creation_helpers::register, wire, *};
use soroban_sdk::{
    xdr::{ScString, ScVal},
    Error, IntoVal, Symbol, Val,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct Step {
    pub op: u8,
    pub field: u8,
    pub index: u8,
    pub native: i128,
    pub bps: u32,
}

fn mutation(
    e: &Env,
    id: &Address,
    admin: &Address,
    target: Option<&Address>,
    field: usize,
    value: i128,
    old: i128,
) -> bool {
    let names = if target.is_some() {
        ["set_native_user_claim_fee", "set_project_claim_fee", "set_remove_fee"]
    } else {
        ["set_default_native_claim_fee", "set_default_project_claim_fee", "set_default_remove_fee"]
    };
    let number: Val = if field == 0 { value.into_val(e) } else { (value as u32).into_val(e) };
    let mut args = soroban_sdk::vec![e, admin.clone().into_val(e)];
    if let Some(project) = target {
        args.push_back(project.clone().into_val(e));
    }
    args.push_back(number);
    authorize(e, id, admin, names[field], args.clone());
    let before = e.to_ledger_snapshot().ledger_entries;
    let actual = e.try_invoke_contract::<(), Error>(id, &Symbol::new(e, names[field]), args);
    let accepted = value != old && value >= 0 && (field == 0 || value <= 10_000);
    assert_eq!(
        actual,
        if accepted { Ok(Ok(())) } else { Err(Ok(Error::from_contract_error(6201))) }
    );
    if accepted {
        let topics = if target.is_some() {
            ["native_claim_fee_updated", "project_claim_fee_updated", "remove_fee_updated"]
        } else {
            [
                "default_native_claim_fee_updated",
                "DefaultProjectClaimFeeUpdated",
                "default_remove_fee_updated",
            ]
        };
        let fields = if target.is_some() {
            ["native_claim_fee", "project_claim_fee", "remove_fee"]
        } else {
            ["new_default_native_claim_fee", "new_project_claim_fee", "default_remove_fee"]
        };
        let mut data = std::vec![(
            fields[field],
            if field == 0 { wire::signed(value) } else { ScVal::U32(value as u32) }
        )];
        if let Some(project) = target {
            data.push(("project_address", wire::address(project)));
        }
        wire::event(e, id, &[wire::symbol(topics[field])], wire::map(&data));
    } else {
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
    }
    accepted
}

pub(super) fn exercise(steps: &[Step], guest: bool) -> u64 {
    let e = test_env();
    e.cost_estimate().budget().reset_unlimited();
    let admin = Address::generate(&e);
    let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
    let id = register(&e, guest, &code, &admin);
    let c = FuulFactoryClient::new(&e, &id);
    let collectors = [admin.clone(), Address::generate(&e), Address::generate(&e)];
    let mut collector = 0;
    let mut defaults = [20_000_i128, 100, 0];
    let mut addresses = std::vec![Address::generate(&e), Address::generate(&e)];
    let mut records = std::vec![([0_i128; 3], false); 2];
    let uri = String::from_str(&e, "ipfs://wide-fees");
    addresses.push(c.create_fuul_project(&admin, &uri, &false));
    records.push((defaults, true));
    let mut counter = 1_u128;
    let mut seen = 0_u64;
    for step in steps {
        let field = usize::from(step.field);
        let index = usize::from(step.index) % addresses.len();
        match step.op {
            0 | 1 => {
                let value = if field == 0 { step.native } else { i128::from(step.bps) };
                let old = if step.op == 0 { defaults[field] } else { records[index].0[field] };
                let accepted = mutation(
                    &e,
                    &id,
                    &admin,
                    (step.op == 1).then_some(&addresses[index]),
                    field,
                    value,
                    old,
                );
                if accepted {
                    seen |= 1 << step.op;
                    if field == 0 && value > i128::from(u64::MAX) {
                        seen |= 1 << 7;
                    }
                    if field != 0 && value == 10_000 {
                        seen |= 1 << 10;
                    }
                    if step.op == 0 {
                        defaults[field] = value;
                    } else {
                        if !records[index].1 {
                            seen |= 1 << 11;
                        }
                        records[index].0[field] = value;
                        records[index].1 = true;
                    }
                } else {
                    seen |= if value == old { 1 << 3 } else { 1 << 2 };
                    if value < 0 {
                        seen |= 1 << 8;
                    }
                    if field != 0 && value == i128::from(u32::MAX) {
                        seen |= 1 << 9;
                    }
                }
            }
            2 => {
                let next = usize::from(step.index) % collectors.len();
                let args = (&admin, &collectors[next]).into_val(&e);
                authorize(&e, &id, &admin, "set_fee_collector", args);
                let before = e.to_ledger_snapshot().ledger_entries;
                let actual = c.try_set_fee_collector(&admin, &collectors[next]);
                if next == collector {
                    assert_eq!(actual, Err(Ok(Error::from_contract_error(6201))));
                    assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
                    assert!(e.events().all().events().is_empty());
                    seen |= 1 << 5;
                } else {
                    assert_eq!(actual, Ok(Ok(())));
                    wire::event(
                        &e,
                        &id,
                        &[wire::symbol("fee_collector_updated"), wire::address(&collectors[next])],
                        wire::map(&[]),
                    );
                    collector = next;
                    seen |= 1 << 4;
                }
            }
            3 => {
                e.mock_auths(&[]);
                let child = c.create_fuul_project(&admin, &uri, &false);
                assert!(!addresses.contains(&child));
                counter += 1;
                wire::event(
                    &e,
                    &id,
                    &[wire::symbol("project_created"), wire::address(&child)],
                    wire::map(&[
                        ("project_id", wire::unsigned(counter)),
                        (
                            "project_info_uri",
                            ScVal::String(ScString("ipfs://wide-fees".try_into().unwrap())),
                        ),
                    ]),
                );
                addresses.push(child);
                records.push((defaults, true));
                seen |= 1 << 6;
            }
            _ => unreachable!(),
        }
        assert_eq!(
            [
                c.default_native_claim_fee(),
                i128::from(c.default_project_claim_fee()),
                i128::from(c.default_remove_fee())
            ],
            defaults
        );
        assert_eq!(c.contract_tracker(), counter);
        for (project, &(fees, present)) in addresses.iter().zip(&records) {
            let info = c.get_fees_information(project);
            assert_eq!(info.fee_collector, collectors[collector]);
            assert_eq!(
                [
                    info.fees.native_user_claim_fee,
                    i128::from(info.fees.project_claim_fee),
                    i128::from(info.fees.remove_fee)
                ],
                fees
            );
            e.as_contract(&id, || {
                assert_eq!(e.storage().persistent().has(&wire::fee_key(&e, project)), present)
            });
        }
    }
    seen
}

pub(super) fn controls() -> std::vec::Vec<Step> {
    let mut steps = std::vec![];
    for op in 0..2 {
        for value in [0, 1, i128::MAX, i128::MAX, -1, i128::MIN, 0] {
            steps.push(Step { op, field: 0, index: 0, native: value, bps: 0 });
        }
        for field in 1..3 {
            for bps in [0, 1, 9_999, 10_000, 10_000, 10_001, u32::MAX, 0] {
                steps.push(Step { op, field, index: 1, native: 0, bps });
            }
        }
        steps.push(Step { op: 3, field: 0, index: 0, native: 0, bps: 0 });
    }
    for index in [1, 1, 2, 0] {
        steps.push(Step { op: 2, field: 0, index, native: 0, bps: 0 });
    }
    steps
}
