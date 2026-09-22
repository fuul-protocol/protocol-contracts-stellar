use crate::test::*;
use soroban_sdk::{
    xdr::{ContractEventBody, Int128Parts, ScMap, ScMapEntry, ScVal, UInt128Parts},
    IntoVal, Symbol, TryFromVal, Val,
};

pub(super) fn symbol(value: &str) -> ScVal {
    ScVal::Symbol(value.try_into().unwrap())
}
pub(super) fn address(value: &Address) -> ScVal {
    ScVal::Address(value.into())
}
pub(super) fn signed(value: i128) -> ScVal {
    ScVal::I128(Int128Parts { hi: (value >> 64) as i64, lo: value as u64 })
}
pub(super) fn unsigned(value: u128) -> ScVal {
    ScVal::U128(UInt128Parts { hi: (value >> 64) as u64, lo: value as u64 })
}
pub(super) fn map(fields: &[(&str, ScVal)]) -> ScVal {
    let mut entries: std::vec::Vec<_> = fields
        .iter()
        .map(|(key, value)| ScMapEntry { key: symbol(key), val: value.clone() })
        .collect();
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    ScVal::Map(Some(ScMap(entries.try_into().unwrap())))
}
pub(super) fn fees(values: [i128; 3]) -> ScVal {
    map(&[
        ("native_user_claim_fee", signed(values[0])),
        ("project_claim_fee", ScVal::U32(values[1] as u32)),
        ("remove_fee", ScVal::U32(values[2] as u32)),
    ])
}
pub(super) fn key(e: &Env, name: &str) -> Val {
    (Symbol::new(e, name),).into_val(e)
}
pub(super) fn fee_key(e: &Env, project: &Address) -> Val {
    (Symbol::new(e, "ProjectFees"), project).into_val(e)
}
pub(super) fn result(e: &Env, id: &Address, name: &str, args: Vec<Val>) -> ScVal {
    let value: Val = e.invoke_contract(id, &Symbol::new(e, name), args);
    ScVal::try_from_val(e, &value).unwrap()
}
pub(super) fn event(e: &Env, id: &Address, topics: &[ScVal], data: ScVal) {
    let events = e.events().all().filter_by_contract(id);
    assert_eq!(events.events().len(), 1);
    let ContractEventBody::V0(body) = &events.events()[0].body;
    assert_eq!(body.topics.as_slice(), topics);
    assert_eq!(body.data, data);
}
