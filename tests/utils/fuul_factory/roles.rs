use crate::test::{authority::*, creation_helpers::register, *};
use fuul_core::INSTANCE_EXTEND_AMOUNT;
use soroban_sdk::{
    testutils::Ledger,
    xdr::{ScErrorCode, ScErrorType},
    Error, IntoVal, Symbol, Val,
};

/// Operation (seven setters, grant, revoke, renounce), actor, member, role, auth mode.
pub(super) type Step = (u8, u8, u8, u8, u8);

pub(super) fn exercise(steps: &[Step], guest: bool) -> u64 {
    let e = test_env();
    e.cost_estimate().budget().reset_unlimited();
    e.ledger().with_mut(|ledger| ledger.min_persistent_entry_ttl = INSTANCE_EXTEND_AMOUNT + 1);
    let actors = [Address::generate(&e), Address::generate(&e), Address::generate(&e)];
    let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
    let id = register(&e, guest, &code, &actors[0]);
    let other = register(&e, guest, &code, &actors[0]);
    let access = FuulAccessControlClient::new(&e, &id);
    let client = FuulFactoryClient::new(&e, &id);
    let roles = [role(&e), Symbol::new(&e, "manager")];
    let mut members = [std::vec![0_usize], std::vec![0_usize]];
    let project = Address::generate(&e);
    e.ledger().with_mut(|ledger| ledger.sequence_number += 70 * 17_280);
    let other_before = state(&e, &other);
    let mut seen = 0_u64;
    for (index, &(op, actor, member, role, auth)) in steps.iter().enumerate() {
        let (actor, member, role) = (usize::from(actor), usize::from(member), usize::from(role));
        let collector = Address::generate(&e);
        let value = index as u32 + 1;
        let (name, args): (&str, Vec<Val>) = match op {
            0..=6 => {
                setters(&e, &actors[actor], &project, &collector, value)[usize::from(op)].clone()
            }
            7 => ("grant_role", (&roles[role], &actors[member], &actors[actor]).into_val(&e)),
            8 => ("revoke_role", (&roles[role], &actors[member], &actors[actor]).into_val(&e)),
            9 => ("renounce_role", (&roles[role], &actors[actor]).into_val(&e)),
            _ => unreachable!(),
        };
        e.mock_auths(&[]);
        if auth != 1 {
            // A correctly signed invocation for another Factory must not authorize this one.
            authorize(&e, if auth == 2 { &other } else { &id }, &actors[actor], name, args.clone());
        }
        let permitted = op == 9 || members[0].contains(&actor);
        let before = e.to_ledger_snapshot().ledger_entries;
        let actual = e.try_invoke_contract::<(), Error>(&id, &Symbol::new(&e, name), args);
        let expected = if auth != 0 {
            Some(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction))
        } else if !permitted {
            Some(Error::from_contract_error(2000))
        } else {
            None
        };
        assert_eq!(
            actual,
            expected.map_or(Ok(Ok(())), |error| Err(Ok(error))),
            "step {index}: {:?}",
            steps[index]
        );
        if let Some(error) = expected {
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
            seen |= if error == Error::from_contract_error(2000) { 1 << 1 } else { 1 << 2 };
            if !members[0].contains(&actor) && members[1].contains(&actor) && auth == 0 && op < 7 {
                seen |= 1 << 8;
            }
        } else if op < 7 {
            seen |= 1 | (1 << (16 + op));
            assert_eq!(e.events().all().filter_by_contract(&id).events().len(), 1);
            match op {
                0 => assert_eq!(client.fee_collector(), collector),
                1 => assert_eq!(client.default_native_claim_fee(), i128::from(value)),
                2 => assert_eq!(
                    client.project_fees(&project).native_user_claim_fee,
                    i128::from(value)
                ),
                3 => assert_eq!(client.default_project_claim_fee(), value),
                4 => assert_eq!(client.project_fees(&project).project_claim_fee, value),
                5 => assert_eq!(client.default_remove_fee(), value),
                6 => assert_eq!(client.project_fees(&project).remove_fee, value),
                _ => unreachable!(),
            }
        } else {
            let target = if op == 9 { actor } else { member };
            let position = members[role].iter().position(|&entry| entry == target);
            let changed = if op == 7 {
                if position.is_none() {
                    members[role].push(target);
                    true
                } else {
                    false
                }
            } else if let Some(position) = position {
                members[role].swap_remove(position);
                true
            } else {
                false
            };
            seen |= 1 << (op - 4);
            if changed {
                let topic = if op == 7 { "role_granted" } else { "role_revoked" };
                wire::event(
                    &e,
                    &id,
                    &[
                        wire::symbol(topic),
                        wire::symbol(if role == 0 { "default_admin" } else { "manager" }),
                        wire::address(&actors[target]),
                    ],
                    wire::map(&[("caller", wire::address(&actors[actor]))]),
                );
            } else {
                seen |= 1 << 6;
                assert!(e.events().all().events().is_empty());
            }
        }
        if members[0].is_empty() {
            seen |= 1 << 7;
        }
        for role in 0..2 {
            let expected: Vec<Address> = Vec::from_slice(
                &e,
                &members[role]
                    .iter()
                    .map(|&index| actors[index].clone())
                    .collect::<std::vec::Vec<_>>(),
            );
            assert_eq!(access.get_role_members(&roles[role]), expected);
            assert_eq!(access.get_role_member_count(&roles[role]), members[role].len() as u32);
            for (actor, address) in actors.iter().enumerate() {
                assert_eq!(access.has_role(&roles[role], address), members[role].contains(&actor));
                if role == 1 {
                    assert_eq!(client.has_manager_role(address), members[role].contains(&actor));
                }
            }
        }
        assert_eq!(state(&e, &other), other_before, "other Factory role/storage scope");
    }
    seen
}

pub(super) fn controls() -> std::vec::Vec<Step> {
    let mut steps = std::vec![(7, 0, 2, 1, 0)]; // Manager-only member is never an administrator.
    for setter in 0..7 {
        for (actor, auth) in [(2, 0), (0, 1), (0, 2), (0, 0)] {
            steps.push((setter, actor, 0, 0, auth));
        }
    }
    steps.extend([(7, 0, 1, 0, 0), (8, 1, 0, 0, 0)]);
    for setter in 0..7 {
        steps.push((setter, 0, 0, 0, 0));
    }
    steps.extend([
        (7, 1, 0, 0, 0),
        (9, 1, 0, 0, 0),
        (8, 0, 1, 0, 0),
        (9, 1, 0, 0, 1),
        (9, 1, 0, 0, 2),
        (9, 1, 0, 0, 0),
        (8, 0, 0, 1, 0),
        (9, 0, 0, 0, 0),
        (0, 0, 0, 0, 0),
    ]);
    steps
}
