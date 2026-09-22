use crate::test::*;
use soroban_sdk::{
    contract, contractimpl, symbol_short, Bytes, Error, IntoVal, Symbol, TryFromVal, Val,
};

pub(super) const MAX_TRACKER: u128 = (1_u128 << 96) - 1;
pub(super) const FACTORY_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_factory.wasm"
));

pub(super) const PROBE_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_project_constructor_fixture.wasm"
));

#[contract]
pub(crate) struct Gate;

#[contractimpl]
impl Gate {
    pub fn ready(e: Env) -> bool {
        e.storage().instance().get(&symbol_short!("ready")).unwrap_or(false)
    }
    pub fn repair(e: Env) {
        e.storage().instance().set(&symbol_short!("ready"), &true);
    }
}

/// Predicts the address from an independent big-endian salt encoding.
pub(super) fn predicted(e: &Env, factory: &Address, tracker: u128) -> Address {
    let len = if tracker <= u128::from(u64::MAX) { 8 } else { 16 };
    let mut bytes = std::vec![0_u8; len];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ((tracker >> (8 * (len - index - 1))) & 255) as u8;
    }
    let salt: BytesN<32> = e.crypto().sha256(&Bytes::from_slice(e, &bytes)).into();
    e.deployer().with_address(factory.clone(), salt).deployed_address()
}

pub(super) fn seed_tracker(e: &Env, id: &Address, value: u128) {
    e.as_contract(id, || e.storage().instance().set(&(Symbol::new(e, "ContractTracker"),), &value));
}

type TrackerResult = Result<
    Result<u128, <u128 as soroban_sdk::TryFromVal<Env, Val>>::Error>,
    Result<Error, soroban_sdk::InvokeError>,
>;

pub(super) fn wire_tracker(e: &Env, id: &Address) -> TrackerResult {
    e.try_invoke_contract::<u128, Error>(id, &Symbol::new(e, "contract_tracker"), Vec::new(e))
}

pub(super) fn register(
    e: &Env,
    compiled: bool,
    project_hash: &BytesN<32>,
    admin: &Address,
) -> Address {
    let args = (admin.clone(), admin.clone(), admin.clone(), project_hash.clone());
    if compiled {
        e.register(FACTORY_WASM, args)
    } else {
        e.register(FuulFactory, args)
    }
}

pub(super) fn assert_domain(e: &Env, id: &Address, admin: &Address) {
    for tracker in [
        0,
        u128::from(u64::MAX) - 1,
        u128::from(u64::MAX),
        u128::from(u64::MAX) + 1,
        MAX_TRACKER - 1,
        MAX_TRACKER,
    ] {
        seed_tracker(e, id, tracker);
        assert_eq!(wire_tracker(e, id), Ok(Ok(tracker)));
        let next = if tracker == MAX_TRACKER { 0 } else { tracker + 1 };
        let uri = String::from_str(e, "ipfs://uint96");
        let result = e.try_invoke_contract::<Address, Error>(
            id,
            &Symbol::new(e, "create_fuul_project"),
            (admin, &uri, false).into_val(e),
        );
        assert_eq!(result, Ok(Ok(predicted(e, id, tracker))), "tracker {tracker}");
        assert_eq!(
            e.events().all().filter_by_contract(id),
            std::vec![ProjectCreated {
                project_id: next,
                deployed_address: predicted(e, id, tracker),
                project_info_uri: uri
            }
            .to_xdr(e, id)]
        );
        assert_eq!(wire_tracker(e, id), Ok(Ok(next)));
    }
    // Salt zero is already occupied after the first creation.
    let before = authority::state(e, id);
    assert!(e
        .try_invoke_contract::<Address, Error>(
            id,
            &Symbol::new(e, "create_fuul_project"),
            (admin, String::from_str(e, "ipfs://wrapped-collision"), false).into_val(e)
        )
        .is_err());
    assert_eq!(authority::state(e, id), before);
    assert!(e.events().all().events().is_empty());
    assert_eq!(wire_tracker(e, id), Ok(Ok(0)));
}

// Accept either stored width to isolate the u64 boundary behavior.
pub(super) fn assert_crosses_u64(e: &Env, id: &Address, admin: &Address) {
    let current = e.invoke_contract::<Val>(id, &Symbol::new(e, "contract_tracker"), Vec::new(e));
    e.as_contract(id, || {
        let key = (Symbol::new(e, "ContractTracker"),);
        if u128::try_from_val(e, &current).is_ok() {
            e.storage().instance().set(&key, &u128::from(u64::MAX));
        } else {
            e.storage().instance().set(&key, &u64::MAX);
        }
    });
    assert_eq!(
        e.try_invoke_contract::<Address, Error>(
            id,
            &Symbol::new(e, "create_fuul_project"),
            (admin, String::from_str(e, "ipfs://above-u64"), false).into_val(e)
        ),
        Ok(Ok(predicted(e, id, u128::from(u64::MAX))))
    );
    assert_eq!(wire_tracker(e, id), Ok(Ok(u128::from(u64::MAX) + 1)));
}
