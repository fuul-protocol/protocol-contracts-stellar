use crate::test::*;
use fuul_core::{INSTANCE_EXTEND_AMOUNT as EXTEND, INSTANCE_TTL_THRESHOLD as THRESHOLD};
use soroban_sdk::{
    testutils::{Deployer, EnvTestConfig},
    xdr::{LedgerEntry, LedgerKey, ScAddress, ScVal},
    Error, TryFromVal, Val,
};

fn entries(e: &Env, id: &Address) -> std::vec::Vec<(LedgerKey, LedgerEntry, Option<u32>)> {
    let owner: ScAddress = id.into();
    e.to_ledger_snapshot()
        .ledger_entries
        .into_iter()
        .filter_map(|(key, (entry, ttl))| {
            matches!(key.as_ref(), LedgerKey::ContractData(data) if data.contract == owner)
                .then_some((*key, *entry, ttl))
        })
        .collect()
}

fn proof_key(e: &Env, p: &BytesN<32>) -> ScVal {
    let key: Val = (Symbol::new(e, "ClaimedProof"), p).into_val(e);
    ScVal::try_from_val(e, &key).unwrap()
}

fn proof_deadline(e: &Env, id: &Address, p: &BytesN<32>) -> u32 {
    let key = proof_key(e, p);
    entries(e, id)
        .into_iter()
        .find_map(|(entry, _, ttl)| {
            matches!(entry, LedgerKey::ContractData(data) if data.key == key)
                .then_some(ttl.unwrap())
        })
        .unwrap()
}

pub(super) fn exercise(boundary: u8, query: u8, extra: u32, bytes: [u8; 32], guest: bool) -> u64 {
    let e = Env::new_with_config(EnvTestConfig { capture_snapshot_at_drop: false });
    e.ledger().with_mut(|ledger| ledger.min_persistent_entry_ttl = EXTEND + 1);
    let f = guest::fixture(&e, guest, 0, 0);
    let id = &f.client.address;
    let factory = f.client.factory();
    let p = BytesN::from_array(&e, &bytes);
    let mut sibling = bytes;
    sibling[0] ^= 1;
    let sibling = BytesN::from_array(&e, &sibling);
    let mut missing = bytes;
    missing[0] ^= 2;
    let missing = BytesN::from_array(&e, &missing);
    for proof in [&p, &sibling] {
        let args = (
            &f.manager,
            &f.recipient,
            &f.currency,
            TokenType::StellarAsset,
            0_i128,
            u(&e, 0),
            proof,
            false,
        )
            .into_val(&e);
        guest::authorize(&e, id, &f.manager, "claim", args);
        assert_eq!(
            f.client.claim(
                &f.manager,
                &f.recipient,
                &f.currency,
                &TokenType::StellarAsset,
                &0,
                &u(&e, 0),
                proof,
                &false
            ),
            ProjectClaimResult { native_user_claim_fee: 0, fee_collector: f.collector.clone() }
        );
    }
    let initial = proof_deadline(&e, id, &p) - e.ledger().sequence();
    if boundary < 4 {
        let remaining = [THRESHOLD - 1, THRESHOLD, THRESHOLD + 1, 0][usize::from(boundary)];
        e.ledger().with_mut(|ledger| ledger.sequence_number += initial - remaining);
    } else {
        e.ledger().with_mut(|ledger| ledger.sequence_number += initial - 1);
        f.client.keep_alive();
        e.ledger()
            .with_mut(|ledger| ledger.sequence_number += if boundary == 4 { 2 } else { extra + 2 });
    }
    let deadline = proof_deadline(&e, id, &p);
    assert_eq!(deadline < e.ledger().sequence(), boundary >= 4);
    let before = entries(&e, id);
    let instance = e.deployer().get_contract_instance_ttl(id);
    let code = e.deployer().get_contract_code_ttl(id);
    e.mock_auths(&[]);
    match query {
        0 => assert!(f.client.claimed_proofs(&p)),
        1 => f.client.keep_alive(),
        2 => assert!(!f.client.claimed_proofs(&missing)),
        _ => unreachable!(),
    }
    assert!(e.events().all().events().is_empty());
    let renewed = if query == 1 && instance <= THRESHOLD { EXTEND } else { instance };
    assert_eq!(e.deployer().get_contract_instance_ttl(id), renewed);
    assert_eq!(
        e.deployer().get_contract_code_ttl(id),
        if query == 1 && code <= THRESHOLD { EXTEND } else { code }
    );
    let current = proof_deadline(&e, id, &p);
    if query != 0 {
        assert_eq!(current, deadline);
    } else if boundary >= 4 {
        assert!(current > e.ledger().sequence());
    } else {
        assert_eq!(
            current,
            if deadline - e.ledger().sequence() <= THRESHOLD {
                e.ledger().sequence() + EXTEND
            } else {
                deadline
            }
        );
    }
    let selected = proof_key(&e, &p);
    let after = entries(&e, id);
    for (key, value, ttl) in &before {
        let (_, next_value, next_ttl) =
            after.iter().find(|(next_key, _, _)| next_key == key).unwrap();
        assert_eq!(value, next_value);
        if let LedgerKey::ContractData(data) = key {
            if data.key != ScVal::LedgerKeyContractInstance && !(query == 0 && data.key == selected)
            {
                assert_eq!(ttl, next_ttl);
            }
        }
    }
    e.as_contract(id, || {
        assert!(!e.storage().persistent().has(&(Symbol::new(&e, "ClaimedProof"), &missing)))
    });
    // Warm archival restoration separately; failed business calls must not absorb its effects.
    assert!(f.client.claimed_proofs(&p));
    assert!(MockFactoryClient::new(&e, &factory).has_manager_role(&f.manager));
    f.client.keep_alive();
    let args = (
        &f.manager,
        &f.recipient,
        &f.currency,
        TokenType::StellarAsset,
        0_i128,
        u(&e, 0),
        &p,
        false,
    )
        .into_val(&e);
    guest::authorize(&e, id, &f.manager, "claim", args);
    let before = e.to_ledger_snapshot().ledger_entries;
    assert_eq!(
        f.client.try_claim(
            &f.manager,
            &f.recipient,
            &f.currency,
            &TokenType::StellarAsset,
            &0,
            &u(&e, 0),
            &p,
            &false
        ),
        Err(Ok(Error::from_contract_error(6102)))
    );
    assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
    assert!(e.events().all().events().is_empty());
    assert!(f.client.claimed_proofs(&p));
    (1 << boundary) | (1 << (8 + query))
}
