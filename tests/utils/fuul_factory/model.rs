use crate::test::{authority::*, creation_helpers::register, wire, *};
use soroban_sdk::{
    xdr::{ScString, ScVal},
    Error, IntoVal, Symbol, Val,
};

#[derive(Clone, Debug)]
pub(super) enum Action {
    Create(bool),
    Default(u8, u32),
    Override(u8, u8, u32),
    Collector(u8),
    Read(u8),
    Denied,
}

pub(super) fn exercise(actions: &[Action], compiled: bool) {
    let e = test_env();
    let admin = Address::generate(&e);
    let outsider = Address::generate(&e);
    let hash = e.deployer().upload_contract_wasm(PROJECT_WASM);
    let id = register(&e, compiled, &hash, &admin);
    let c = FuulFactoryClient::new(&e, &id);
    let collectors = [admin.clone(), Address::generate(&e), Address::generate(&e)];
    let mut collector = 0;
    let mut defaults = [20_000_i128, 100, 0];
    let mut addresses = std::vec![Address::generate(&e), Address::generate(&e)];
    let mut records = std::vec![([0_i128; 3], false); 2];
    let mut count = 0_u128;
    e.mock_all_auths();
    for action in actions {
        let before = state(&e, &id);
        let mut expected_event = None;
        match *action {
            Action::Create(valid) => {
                let uri = String::from_str(&e, if valid { "ipfs://model" } else { "" });
                let result = c.try_create_fuul_project(&admin, &uri, &false);
                if valid {
                    let project = result.unwrap().unwrap();
                    assert!(!addresses.contains(&project));
                    addresses.push(project.clone());
                    records.push((defaults, true));
                    count += 1;
                    expected_event = Some((
                        std::vec![wire::symbol("project_created"), wire::address(&project)],
                        wire::map(&[
                            ("project_id", wire::unsigned(count)),
                            (
                                "project_info_uri",
                                ScVal::String(ScString("ipfs://model".try_into().unwrap())),
                            ),
                        ]),
                    ));
                } else {
                    assert_eq!(result, Err(Ok(Error::from_contract_error(6200))));
                    assert_eq!(state(&e, &id), before);
                }
            }
            Action::Default(field, value) | Action::Override(_, field, value) => {
                let field = usize::from(field) % 3;
                let target = match *action {
                    Action::Override(index, _, _) => Some(usize::from(index) % addresses.len()),
                    _ => None,
                };
                let prior = target.map(|i| records[i].0[field]).unwrap_or(defaults[field]);
                let accepted = i128::from(value) != prior && (field == 0 || value <= 10_000);
                let names = if target.is_some() {
                    ["set_native_user_claim_fee", "set_project_claim_fee", "set_remove_fee"]
                } else {
                    [
                        "set_default_native_claim_fee",
                        "set_default_project_claim_fee",
                        "set_default_remove_fee",
                    ]
                };
                let number: Val =
                    if field == 0 { i128::from(value).into_val(&e) } else { value.into_val(&e) };
                let mut args = Vec::new(&e);
                args.push_back(admin.clone().into_val(&e));
                if let Some(index) = target {
                    args.push_back(addresses[index].clone().into_val(&e));
                }
                args.push_back(number);
                let result =
                    e.try_invoke_contract::<(), Error>(&id, &Symbol::new(&e, names[field]), args);
                assert_eq!(
                    result,
                    if accepted { Ok(Ok(())) } else { Err(Ok(Error::from_contract_error(6201))) },
                    "{action:?}"
                );
                if accepted {
                    if let Some(index) = target {
                        records[index].0[field] = i128::from(value);
                        records[index].1 = true;
                    } else {
                        defaults[field] = i128::from(value);
                    }
                    let topics = if target.is_some() {
                        [
                            "native_claim_fee_updated",
                            "project_claim_fee_updated",
                            "remove_fee_updated",
                        ]
                    } else {
                        [
                            "default_native_claim_fee_updated",
                            "DefaultProjectClaimFeeUpdated",
                            "default_remove_fee_updated",
                        ]
                    };
                    let field_names = if target.is_some() {
                        ["native_claim_fee", "project_claim_fee", "remove_fee"]
                    } else {
                        [
                            "new_default_native_claim_fee",
                            "new_project_claim_fee",
                            "default_remove_fee",
                        ]
                    };
                    let mut fields = std::vec![(
                        field_names[field],
                        if field == 0 {
                            wire::signed(i128::from(value))
                        } else {
                            ScVal::U32(value)
                        }
                    )];
                    if let Some(index) = target {
                        fields.push(("project_address", wire::address(&addresses[index])));
                    }
                    expected_event =
                        Some((std::vec![wire::symbol(topics[field])], wire::map(&fields)));
                } else {
                    assert_eq!(state(&e, &id), before);
                }
            }
            Action::Collector(index) => {
                let next = usize::from(index) % collectors.len();
                let result = c.try_set_fee_collector(&admin, &collectors[next]);
                if next == collector {
                    assert_eq!(result, Err(Ok(Error::from_contract_error(6201))));
                    assert_eq!(state(&e, &id), before);
                } else {
                    assert_eq!(result, Ok(Ok(())));
                    collector = next;
                    expected_event = Some((
                        std::vec![
                            wire::symbol("fee_collector_updated"),
                            wire::address(&collectors[next])
                        ],
                        wire::map(&[]),
                    ));
                }
            }
            Action::Read(index) => {
                let index = usize::from(index) % addresses.len();
                let actual = c.project_fees(&addresses[index]);
                assert_eq!(
                    [
                        actual.native_user_claim_fee,
                        i128::from(actual.project_claim_fee),
                        i128::from(actual.remove_fee)
                    ],
                    records[index].0
                );
            }
            Action::Denied => {
                assert_eq!(
                    c.try_set_default_native_claim_fee(&outsider, &1),
                    Err(Ok(Error::from_contract_error(2000)))
                );
                assert_eq!(state(&e, &id), before);
            }
        }
        if let Some((topics, data)) = expected_event {
            wire::event(&e, &id, &topics, data);
        } else {
            assert!(e.events().all().events().is_empty(), "{action:?}");
        }
        assert_eq!(c.contract_tracker(), count);
        assert_eq!(
            [
                c.default_native_claim_fee(),
                i128::from(c.default_project_claim_fee()),
                i128::from(c.default_remove_fee())
            ],
            defaults
        );
        for (index, project) in addresses.iter().enumerate() {
            let info = c.get_fees_information(project);
            assert_eq!(info.fee_collector, collectors[collector]);
            assert_eq!(
                [
                    info.fees.native_user_claim_fee,
                    i128::from(info.fees.project_claim_fee),
                    i128::from(info.fees.remove_fee)
                ],
                records[index].0
            );
            e.as_contract(&id, || {
                assert_eq!(
                    e.storage().persistent().has(&super::wire::fee_key(&e, project)),
                    records[index].1
                )
            });
        }
    }
}
