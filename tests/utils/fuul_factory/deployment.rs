use crate::test::{authority::*, creation_helpers::*, wire, *};
use soroban_sdk::{
    testutils::Ledger,
    xdr::{self, ContractEventBody, ScError, ScErrorCode, ScVal, WriteXdr},
    Bytes, Error, IntoVal, Symbol, TryFromVal, Val,
};

/// Independently constructs the network-bound XDR preimage, not a Deployer address oracle.
fn address(e: &Env, factory: &Address, counter: u128) -> Address {
    let width = if counter <= u128::from(u64::MAX) { 8 } else { 16 };
    let salt_bytes: std::vec::Vec<_> =
        (0..width).rev().map(|i| ((counter >> (i * 8)) & 255) as u8).collect();
    let salt = e.crypto().sha256(&Bytes::from_slice(e, &salt_bytes)).to_array();
    let preimage = xdr::HashIdPreimage::ContractId(xdr::HashIdPreimageContractId {
        network_id: xdr::Hash(e.ledger().network_id().to_array()),
        contract_id_preimage: xdr::ContractIdPreimage::Address(
            xdr::ContractIdPreimageFromAddress {
                address: factory.into(),
                salt: xdr::Uint256(salt),
            },
        ),
    });
    let hash =
        e.crypto().sha256(&Bytes::from_slice(e, &preimage.to_xdr(xdr::Limits::none()).unwrap()));
    Address::try_from_val(e, &xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(hash.to_array()))))
        .unwrap()
}

fn diagnostic(e: &Env, error: ScError) {
    let diagnostics = e.host().get_diagnostic_events().unwrap();
    assert!(
        diagnostics.0.iter().any(|event| {
            let ContractEventBody::V0(body) = &event.event.body;
            event.failed_call && body.topics.contains(&ScVal::Error(error.clone()))
        }),
        "missing {error:?}: {diagnostics:?}"
    );
}

/// Modes: clean, missing code, incompatible ABI, late constructor failure, occupied salt.
/// Counter corruption and restoration to a valid counter are separate fixture operations.
pub(super) fn exercise(counter: u128, network: [u8; 32], mode: u8, guest: bool) -> u64 {
    assert!(counter <= MAX_TRACKER);
    let e = test_env();
    e.cost_estimate().budget().reset_unlimited();
    e.ledger().with_mut(|ledger| ledger.network_id = network);
    let admin = Address::generate(&e);
    // Mocking an address for admin auth can replace its native contract functions.
    // Keep the live constructor gate separate from the mocked Factory administrator.
    let project_admin = if mode == 3 { e.register(Gate, ()) } else { Address::generate(&e) };
    let expected_code: BytesN<32> = e.crypto().sha256(&Bytes::from_slice(&e, PROJECT_WASM)).into();
    let code = match mode {
        1 => expected_code.clone(),
        2 => e.deployer().upload_contract_wasm(FACTORY_WASM),
        3 => e.deployer().upload_contract_wasm(PROBE_WASM),
        _ => e.deployer().upload_contract_wasm(PROJECT_WASM),
    };
    let id = register(&e, guest, &code, &admin);
    let c = FuulFactoryClient::new(&e, &id);
    // Vary the complete fee snapshot, including values far outside the old small alphabet.
    let fees = [counter as i128, i128::from(network[0]) * 39, i128::from(network[1]) * 39];
    let updates: [(&str, Val, i128, i128); 3] = [
        ("set_default_native_claim_fee", fees[0].into_val(&e), 20_000, fees[0]),
        ("set_default_project_claim_fee", (fees[1] as u32).into_val(&e), 100, fees[1]),
        ("set_default_remove_fee", (fees[2] as u32).into_val(&e), 0, fees[2]),
    ];
    for (name, value, old, new) in updates {
        if old != new {
            let args: Vec<Val> = (admin.clone(), value).into_val(&e);
            authorize(&e, &id, &admin, name, args.clone());
            e.invoke_contract::<()>(&id, &Symbol::new(&e, name), args);
        }
    }
    let uri = String::from_str(&e, &std::format!("ipfs://counter/{counter}"));
    let kyc = network[2] & 1 != 0;
    let args: Vec<Val> = (&project_admin, &uri, kyc).into_val(&e);
    e.mock_auths(&[]);
    seed_tracker(&e, &id, counter | (1_u128 << 96));
    let corrupt = e.to_ledger_snapshot().ledger_entries;
    assert_eq!(
        e.try_invoke_contract::<Address, Error>(
            &id,
            &Symbol::new(&e, "create_fuul_project"),
            args.clone()
        ),
        Err(Ok(Error::from_contract_error(6202)))
    );
    assert_eq!(e.to_ledger_snapshot().ledger_entries, corrupt);
    assert!(e.events().all().events().is_empty());
    seed_tracker(&e, &id, counter);
    let expected = address(&e, &id, counter);
    if mode == 4 {
        assert_eq!(c.create_fuul_project(&project_admin, &uri, &kyc), expected);
        seed_tracker(&e, &id, counter);
    }
    if mode != 0 {
        let before = e.to_ledger_snapshot().ledger_entries;
        let result = c.try_create_fuul_project(&project_admin, &uri, &kyc);
        assert!(result.is_err(), "mode {mode}: {result:?}");
        match mode {
            1 => diagnostic(&e, ScError::Storage(ScErrorCode::MissingValue)),
            2 => diagnostic(&e, ScError::WasmVm(ScErrorCode::InvalidAction)),
            3 => diagnostic(&e, ScError::Contract(8200)),
            4 => diagnostic(&e, ScError::Storage(ScErrorCode::ExistingValue)),
            _ => unreachable!(),
        }
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before, "mode {mode}: atomic ledger");
        assert!(e.events().all().events().is_empty());
        assert_eq!(c.contract_tracker(), counter);
        if mode == 4 {
            // Occupied addresses cannot be repaired by overwriting another Project.
            assert!(c.try_create_fuul_project(&project_admin, &uri, &kyc).is_err());
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert_eq!(
                wire::result(&e, &id, "project_fees", (&expected,).into_val(&e)),
                wire::fees(fees)
            );
            seed_tracker(&e, &id, (counter + 1) & MAX_TRACKER);
        } else if mode == 3 {
            GateClient::new(&e, &project_admin).repair();
        } else {
            assert_eq!(e.deployer().upload_contract_wasm(PROJECT_WASM), expected_code);
            if mode == 2 {
                // Test-fixture repair of an invalid immutable configuration, not a public setter.
                e.as_contract(&id, || {
                    e.storage().instance().set(&wire::key(&e, "ProjectWasmHash"), &expected_code)
                });
            }
        }
    }
    let current = if mode == 4 { (counter + 1) & MAX_TRACKER } else { counter };
    let child = c.create_fuul_project(&project_admin, &uri, &kyc);
    assert_eq!(child, address(&e, &id, current));
    let next = (current + 1) & MAX_TRACKER;
    wire::event(
        &e,
        &id,
        &[wire::symbol("project_created"), wire::address(&child)],
        wire::map(&[
            ("project_id", wire::unsigned(next)),
            (
                "project_info_uri",
                ScVal::String(xdr::ScString(
                    std::format!("ipfs://counter/{counter}").try_into().unwrap(),
                )),
            ),
        ]),
    );
    assert_eq!(c.contract_tracker(), next);
    assert_eq!(wire::result(&e, &id, "project_fees", (&child,).into_val(&e)), wire::fees(fees));
    if mode == 3 {
        e.as_contract(&child, || {
            assert_eq!(
                e.storage().instance().get::<_, (Address, String, bool)>(&Symbol::new(&e, "probe")),
                Some((id.clone(), uri.clone(), kyc))
            )
        });
    } else {
        let p = FuulProjectClient::new(&e, &child);
        assert_eq!(p.factory(), id);
        assert_eq!(p.project_info_uri(), uri);
        assert_eq!(p.kyc_required(), kyc);
        let access = FuulAccessControlClient::new(&e, &child);
        assert_eq!(access.get_role_members(&role(&e)), soroban_sdk::vec![&e, project_admin]);
    }
    (1 << mode)
        | if counter > u128::from(u64::MAX) { 1 << 5 } else { 0 }
        | if counter == MAX_TRACKER { 1 << 6 } else { 0 }
}
