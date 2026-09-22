use super::*;
use fuul_core::upgrade::UpgradeableClient;
use soroban_sdk::{
    testutils::{storage::Persistent, Deployer},
    xdr,
};

fn values(
    e: &Env,
    id: &Address,
) -> std::vec::Vec<(xdr::ContractDataDurability, xdr::ScVal, xdr::ScVal)> {
    let address: xdr::ScAddress = id.into();
    let mut result = std::vec::Vec::new();
    for (_, entry) in e.host().get_stored_entries().unwrap() {
        let Some((entry, _)) = entry else { continue };
        let xdr::LedgerEntryData::ContractData(data) = &entry.data else { continue };
        if data.contract != address {
            continue;
        }
        let value = match &data.val {
            xdr::ScVal::ContractInstance(i) => xdr::ScVal::Map(i.storage.clone()),
            value => value.clone(),
        };
        result.push((data.durability, data.key.clone(), value));
    }
    result.sort();
    result
}

#[test]
fn all_upgrades_require_exact_authority_and_preserve_existing_storage() {
    let e = Env::default();
    let f = Fixture::new(&e);
    f.claim(&f.check(40, 100));
    let hash = e.deployer().upload_contract_wasm(REPLACEMENT);
    // Project upgrade authority belongs to the Factory administrator.
    for (id, admin, outsider, code) in [
        (&f.manager.address, &f.admin, &f.caller, 2000),
        (&f.project.address, &f.factory_admin, &f.project_admin, 6101),
        (&f.factory.address, &f.factory_admin, &f.caller, 2000),
    ] {
        let client = UpgradeableClient::new(&e, id);
        let before = values(&e, id);
        e.mock_auths(&[]);
        assert!(client.try_upgrade(&hash, admin).is_err());
        e.mock_all_auths();
        assert_eq!(client.try_upgrade(&hash, outsider), Err(Ok(error(code))));
        assert!(client.try_upgrade(&BytesN::from_array(&e, &[99; 32]), admin).is_err());
        assert_eq!(values(&e, id), before);
        e.mock_auths(&[MockAuth {
            address: admin,
            invoke: &MockAuthInvoke {
                contract: id,
                fn_name: "upgrade",
                args: (&hash, admin).into_val(&e),
                sub_invokes: &[],
            },
        }]);
        client.upgrade(&hash, admin);
        assert_eq!(values(&e, id), before);
        assert_eq!(e.invoke_contract::<u32>(id, &symbol_short!("revision"), vec![&e]), 2);
    }
}

#[test]
fn keep_alive_renews_code_and_instance_without_renewing_every_proof() {
    use fuul_core::{INSTANCE_EXTEND_AMOUNT as EXTEND, INSTANCE_TTL_THRESHOLD as THRESHOLD};
    for remaining in [THRESHOLD - 1, THRESHOLD, THRESHOLD + 1] {
        let e = Env::default();
        e.ledger().with_mut(|l| l.min_persistent_entry_ttl = EXTEND + 1);
        let f = Fixture::new(&e);
        let c = f.check(41, 100);
        f.claim(&c);
        let ttl = e.deployer().get_contract_instance_ttl(&f.project.address);
        e.ledger().with_mut(|l| l.sequence_number += ttl - remaining);
        let key = (Symbol::new(&e, "ClaimedProof"), &c.proof);
        let before = e.as_contract(&f.project.address, || e.storage().persistent().get_ttl(&key));
        e.mock_auths(&[]);
        f.manager.keep_alive();
        f.factory.keep_alive();
        f.project.keep_alive();
        let expected = if remaining <= THRESHOLD { EXTEND } else { remaining };
        for id in [&f.manager.address, &f.factory.address, &f.project.address] {
            assert_eq!(e.deployer().get_contract_instance_ttl(id), expected);
            assert_eq!(e.deployer().get_contract_code_ttl(id), expected);
        }
        assert_eq!(
            e.as_contract(&f.project.address, || e.storage().persistent().get_ttl(&key)),
            before
        );
        assert!(f.project.claimed_proofs(&c.proof));
        assert_eq!(
            e.as_contract(&f.project.address, || e.storage().persistent().get_ttl(&key)),
            if before <= THRESHOLD { EXTEND } else { before }
        );
    }
}
