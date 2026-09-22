use crate::test::*;
use soroban_sdk::{
    testutils::{EnvTestConfig, MockAuth, MockAuthInvoke},
    xdr::{LedgerEntry, LedgerKey, ScAddress},
    IntoVal, Symbol, Val,
};

pub(super) fn test_env() -> Env {
    Env::new_with_config(EnvTestConfig { capture_snapshot_at_drop: false })
}

pub(super) fn authorize(e: &Env, id: &Address, caller: &Address, name: &str, args: Vec<Val>) {
    e.mock_auths(&[MockAuth {
        address: caller,
        invoke: &MockAuthInvoke { contract: id, fn_name: name, args, sub_invokes: &[] },
    }]);
}

pub(super) fn state(e: &Env, id: &Address) -> std::vec::Vec<(LedgerKey, LedgerEntry, Option<u32>)> {
    let owner: ScAddress = id.into();
    let mut entries: std::vec::Vec<_> = e
        .to_ledger_snapshot()
        .ledger_entries
        .into_iter()
        .filter_map(|(key, (entry, ttl))| {
            matches!(key.as_ref(), LedgerKey::ContractData(data) if data.contract == owner)
                .then_some((*key, *entry, ttl))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

pub(super) fn setters(
    e: &Env,
    actor: &Address,
    project: &Address,
    collector: &Address,
    value: u32,
) -> std::vec::Vec<(&'static str, Vec<Val>)> {
    std::vec![
        ("set_fee_collector", (actor, collector).into_val(e)),
        ("set_default_native_claim_fee", (actor, i128::from(value)).into_val(e)),
        ("set_native_user_claim_fee", (actor, project, i128::from(value)).into_val(e)),
        ("set_default_project_claim_fee", (actor, value).into_val(e)),
        ("set_project_claim_fee", (actor, project, value).into_val(e)),
        ("set_default_remove_fee", (actor, value).into_val(e)),
        ("set_remove_fee", (actor, project, value).into_val(e)),
    ]
}

pub(super) fn role(e: &Env) -> Symbol {
    Symbol::new(e, "default_admin")
}
