use crate::test::*;

fn assert_raw_configuration_events(
    e: &Env,
    id: &Address,
    expected: &[(&str, &str, soroban_sdk::xdr::ScVal)],
) {
    use soroban_sdk::xdr::{ContractEventBody, ScMap, ScMapEntry, ScVal};
    let events = e.events().all();
    let project_events = events.filter_by_contract(id);
    let initial = expected.len() == 2;
    let offset = usize::from(initial);
    assert_eq!(events.events().len(), expected.len() + offset);
    assert_eq!(project_events.events().len(), expected.len() + offset);
    for (event, (topic, field, value)) in project_events.events().iter().skip(offset).zip(expected)
    {
        let ContractEventBody::V0(body) = &event.body;
        assert_eq!(body.topics.as_slice(), &[ScVal::Symbol((*topic).try_into().unwrap())]);
        assert_eq!(
            body.data,
            ScVal::Map(Some(ScMap(
                std::vec![ScMapEntry {
                    key: ScVal::Symbol((*field).try_into().unwrap()),
                    val: value.clone(),
                }]
                .try_into()
                .unwrap()
            )))
        );
    }
}

#[test]
fn configuration_events_preserve_raw_order_and_authority_in_native_and_wasm() {
    use soroban_sdk::{
        testutils::{EnvTestConfig, MockAuth, MockAuthInvoke},
        xdr::{ContractEventBody, ScError, ScErrorCode, ScString, ScVal},
        Error, Val,
    };
    const PROJECT_WASM: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/wasm32v1-none/release/fuul_project.wasm"
    ));
    for guest in [false, true] {
        let e = Env::new_with_config(EnvTestConfig { capture_snapshot_at_drop: false });
        e.ledger().with_mut(|l| l.min_persistent_entry_ttl = 90 * 17_280 + 1);
        let admin = Address::generate(&e);
        let outsider = Address::generate(&e);
        let factory = Address::generate(&e);
        let uri = String::from_str(&e, "ipfs://initial");
        let id = Address::generate(&e);
        let args: Vec<Val> = (&factory, &admin, &uri, false).into_val(&e);
        e.mock_auths(&[MockAuth {
            address: &factory,
            invoke: &MockAuthInvoke {
                contract: &id,
                fn_name: "__constructor",
                args: args.clone(),
                sub_invokes: &[],
            },
        }]);
        if guest {
            e.register_at(&id, PROJECT_WASM, args);
        } else {
            e.register_at(&id, FuulProject, args);
        }
        assert_eq!(
            e.events().all().events()[0],
            stellar_access::access_control::RoleGranted {
                role: Symbol::new(&e, "default_admin"),
                account: admin.clone(),
                caller: factory.clone(),
            }
            .to_xdr(&e, &id)
        );
        assert_raw_configuration_events(
            &e,
            &id,
            &[
                (
                    "project_info_updated",
                    "project_info_uri",
                    ScVal::String(ScString("ipfs://initial".try_into().unwrap())),
                ),
                ("kyc_required_updated", "required", ScVal::Bool(false)),
            ],
        );
        e.ledger().with_mut(|l| l.sequence_number += 70 * 17_280);
        let calls: [(&str, Vec<Val>, &str, &str, ScVal); 4] = [
            (
                "set_project_uri",
                (&admin, String::from_str(&e, "ipfs://updated")).into_val(&e),
                "project_info_updated",
                "project_info_uri",
                ScVal::String(ScString("ipfs://updated".try_into().unwrap())),
            ),
            (
                "set_kyc_required",
                (&admin, true).into_val(&e),
                "kyc_required_updated",
                "required",
                ScVal::Bool(true),
            ),
            (
                "set_kyc_required",
                (&admin, false).into_val(&e),
                "kyc_required_updated",
                "required",
                ScVal::Bool(false),
            ),
            (
                "set_kyc_required",
                (&admin, false).into_val(&e),
                "kyc_required_updated",
                "required",
                ScVal::Bool(false),
            ),
        ];
        for (name, args, topic, field, value) in calls {
            for actor in [None, Some(&outsider)] {
                if let Some(actor) = actor {
                    e.mock_auths(&[MockAuth {
                        address: actor,
                        invoke: &MockAuthInvoke {
                            contract: &id,
                            fn_name: name,
                            args: args.clone(),
                            sub_invokes: &[],
                        },
                    }]);
                } else {
                    e.mock_auths(&[]);
                }
                let before = e.to_ledger_snapshot().ledger_entries;
                assert!(e
                    .try_invoke_contract::<(), Error>(&id, &Symbol::new(&e, name), args.clone())
                    .is_err());
                assert!(e.events().all().events().is_empty());
                let diagnostics = e.host().get_diagnostic_events().unwrap();
                assert!(diagnostics.0.iter().any(|event| {
                    let ContractEventBody::V0(body) = &event.event.body;
                    event.failed_call
                        && body
                            .topics
                            .contains(&ScVal::Error(ScError::Auth(ScErrorCode::InvalidAction)))
                }));
                assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            }
            e.mock_auths(&[MockAuth {
                address: &admin,
                invoke: &MockAuthInvoke {
                    contract: &id,
                    fn_name: name,
                    args: args.clone(),
                    sub_invokes: &[],
                },
            }]);
            e.invoke_contract::<()>(&id, &Symbol::new(&e, name), args);
            assert_raw_configuration_events(&e, &id, &[(topic, field, value)]);
        }
        let client = FuulProjectClient::new(&e, &id);
        assert_eq!(client.project_info_uri(), String::from_str(&e, "ipfs://updated"));
        assert!(!client.kyc_required());
        let empty = String::from_str(&e, "");
        e.mock_auths(&[MockAuth {
            address: &admin,
            invoke: &MockAuthInvoke {
                contract: &id,
                fn_name: "set_project_uri",
                args: (&admin, &empty).into_val(&e),
                sub_invokes: &[],
            },
        }]);
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            client.try_set_project_uri(&admin, &empty),
            Err(Ok(Error::from_contract_error(6100)))
        );
        assert!(e.events().all().events().is_empty());
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
    }
}

#[test]
fn admin_updates_the_project_uri() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let uri = String::from_str(&env, "ipfs://updated");

    fixture.client.set_project_uri(&fixture.admin, &uri);

    assert_eq!(fixture.client.project_info_uri(), uri);
}

#[test]
#[should_panic(expected = "Error(Contract, #6100)")]
fn project_uri_update_rejects_an_empty_uri() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_project_uri(&fixture.admin, &String::from_str(&env, ""));
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn project_uri_update_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.set_project_uri(&fixture.admin, &String::from_str(&env, "ipfs://blocked"));
}

#[test]
fn admin_updates_the_kyc_requirement() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();

    fixture.client.set_kyc_required(&fixture.admin, &true);
    assert!(fixture.client.kyc_required());

    fixture.client.set_kyc_required(&fixture.admin, &false);
    assert!(!fixture.client.kyc_required());
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn kyc_requirement_update_requires_admin_authorization() {
    let env = Env::default();
    let fixture = fixture(&env);

    fixture.client.set_kyc_required(&fixture.admin, &true);
}

#[test]
fn configuration_updates_publish_events() {
    let env = Env::default();
    let fixture = fixture(&env);
    env.mock_all_auths();
    let uri = String::from_str(&env, "ipfs://events");

    fixture.client.set_project_uri(&fixture.admin, &uri);

    assert_eq!(
        env.events().all(),
        std::vec![
            ProjectInfoUpdated { project_info_uri: uri }.to_xdr(&env, &fixture.client.address)
        ]
    );

    fixture.client.set_kyc_required(&fixture.admin, &true);

    assert_eq!(
        env.events().all(),
        std::vec![KycRequiredUpdated { required: true }.to_xdr(&env, &fixture.client.address)]
    );
}
