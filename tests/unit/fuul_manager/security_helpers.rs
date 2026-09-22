use crate::test::*;
use soroban_sdk::xdr::LedgerEntry;

// Independent wire oracle: literal EVM fields, without authorization().
pub(super) fn claim_payload(env: &Env, c: &ClaimCheck) -> Vec<Val> {
    use soroban_sdk::Map;
    let fields = Map::<Symbol, Val>::from_array(
        env,
        [
            (Symbol::new(env, "project_address"), c.project_address.clone().into_val(env)),
            (Symbol::new(env, "to"), c.to.clone().into_val(env)),
            (Symbol::new(env, "currency"), c.currency.clone().into_val(env)),
            (Symbol::new(env, "amount"), c.amount.into_val(env)),
            (
                Symbol::new(env, "reason"),
                (Symbol::new(
                    env,
                    match c.reason {
                        ClaimReason::AffiliatePayout => "AffiliatePayout",
                        ClaimReason::EndUserPayout => "EndUserPayout",
                    },
                ),)
                    .into_val(env),
            ),
            (Symbol::new(env, "token_id"), c.token_id.into_val(env)),
            (Symbol::new(env, "deadline"), c.deadline.into_val(env)),
            (Symbol::new(env, "proof"), c.proof.clone().into_val(env)),
        ],
    );
    (fields,).into_val(env)
}

pub(super) fn authorize_claims(
    env: &Env,
    manager: &Address,
    caller: &Address,
    checks: &Vec<ClaimCheck>,
    transfers: &[MockAuthInvoke<'_>],
) {
    let caller_invoke = MockAuthInvoke {
        contract: manager,
        fn_name: "claim",
        args: (caller, checks).into_val(env),
        sub_invokes: transfers,
    };
    let roots: std::vec::Vec<_> = checks
        .iter()
        .flat_map(|check| {
            check
                .signers
                .iter()
                .map(|signer| {
                    (
                        signer,
                        MockAuthInvoke {
                            contract: manager,
                            fn_name: "claim",
                            args: claim_payload(env, &check),
                            sub_invokes: &[],
                        },
                    )
                })
                .collect::<std::vec::Vec<_>>()
        })
        .collect();
    let mut auth = std::vec![MockAuth { address: caller, invoke: &caller_invoke }];
    auth.extend(roots.iter().map(|(signer, root)| MockAuth { address: signer, invoke: root }));
    env.mock_auths(&auth);
}

// SDK Persistent::all includes other contracts. Filter the raw ledger by owner;
// this also avoids opening an as_contract invocation after restrictive auth setup.
pub(super) fn state(
    env: &Env,
    id: &Address,
) -> std::vec::Vec<(LedgerKey, LedgerEntry, Option<u32>)> {
    let owner: ScAddress = id.into();
    let mut entries: std::vec::Vec<_> = env
        .to_ledger_snapshot()
        .ledger_entries
        .into_iter()
        .filter_map(|(key, (entry, live_until))| {
            matches!(key.as_ref(), LedgerKey::ContractData(data) if data.contract == owner)
                .then_some((*key, *entry, live_until))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}
