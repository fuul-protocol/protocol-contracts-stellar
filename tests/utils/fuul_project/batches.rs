use crate::test::*;
use soroban_sdk::{
    testutils::EnvTestConfig,
    xdr::{
        ContractEventBody, ScAddress, ScBytes, ScError, ScErrorCode, ScErrorType, ScString, ScVal,
        ScVec,
    },
    Error, TryFromVal, Val,
};

#[derive(Clone, Debug)]
pub(super) struct Case {
    pub multi: bool,
    pub ids: std::vec::Vec<i128>,
    pub quantities: std::vec::Vec<i128>,
    pub shortage: usize,
}

/// Read the rolled-back call trace immediately after this fresh fixture's only batch.
fn completed_prefix(
    e: &Env,
    f: &ClaimFixture<'_>,
    currency: &Address,
    case: &Case,
    failed_at: usize,
    cause: ScError,
) -> usize {
    let ScAddress::Contract(token) = currency.into() else { unreachable!() };
    let ScAddress::Contract(project) = (&f.client.address).into() else { unreachable!() };
    let symbol = |name: &str| ScVal::Symbol(name.try_into().unwrap());
    let call_topics = [
        symbol("fn_call"),
        ScVal::Bytes(ScBytes(token.0 .0.to_vec().try_into().unwrap())),
        symbol("transfer"),
    ];
    let return_topics = [symbol("fn_return"), symbol("transfer")];
    let diagnostics = e.host().get_diagnostic_events().unwrap();
    let calls: std::vec::Vec<_> = diagnostics
        .0
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            let ContractEventBody::V0(body) = &event.event.body;
            event.event.contract_id.as_ref() == Some(&project)
                && body.topics.as_slice() == call_topics
        })
        .collect();
    assert_eq!(calls.len(), failed_at + 1, "wrong failing transfer: {case:?}");
    let mut completed = 0;
    for (position, &(index, call)) in calls.iter().enumerate() {
        let ContractEventBody::V0(body) = &call.event.body;
        let mut args: Vec<Val> =
            (&f.client.address, &f.recipient, case.ids[position] as u32).into_val(e);
        if case.multi {
            args.push_back(case.quantities[position].into_val(e));
        }
        let args: Val = args.into_val(e);
        let expected_args = ScVal::try_from_val(e, &args).unwrap();
        assert_eq!(body.data, expected_args, "transfer entry {position}");
        assert!(call.failed_call);
        let end = calls.get(position + 1).map_or(diagnostics.0.len(), |(index, _)| *index);
        let frame = &diagnostics.0[index + 1..end];
        let returns: std::vec::Vec<_> = frame
            .iter()
            .filter(|event| {
                let ContractEventBody::V0(body) = &event.event.body;
                event.event.contract_id.as_ref() == Some(&token)
                    && body.topics.as_slice() == return_topics
            })
            .collect();
        assert_eq!(returns.len(), usize::from(position < failed_at));
        if position < failed_at {
            let ContractEventBody::V0(body) = &returns[0].event.body;
            assert_eq!(body.data, ScVal::Void);
            // A successful fn_return survives in diagnostics even when the parent rolls back.
            assert!(returns[0].failed_call);
            completed += 1;
        } else {
            assert!(
                frame.iter().any(|event| {
                    let ContractEventBody::V0(body) = &event.event.body;
                    event.failed_call
                        && event.event.contract_id.as_ref() == Some(&token)
                        && body.topics.as_slice() == [symbol("error"), ScVal::Error(cause.clone())]
                }),
                "missing token-local cause {cause:?}: {case:?}"
            );
            if case.multi {
                let ScVal::Vec(Some(args)) = expected_args else { unreachable!() };
                let mut message = std::vec![ScVal::String(ScString(
                    "caught panic 'insufficient balance' from contract function 'Symbol(transfer)'"
                        .try_into()
                        .unwrap()
                ))];
                message.extend(args.0.iter().cloned());
                let expected = ScVal::Vec(Some(ScVec(message.try_into().unwrap())));
                assert!(
                    frame.iter().any(|event| {
                        let ContractEventBody::V0(body) = &event.event.body;
                        event.failed_call
                            && event.event.contract_id.as_ref() == Some(&token)
                            && body.topics.as_slice() == [symbol("log")]
                            && body.data == expected
                    }),
                    "missing exact insufficient-balance panic and failing arguments: {case:?}"
                );
            }
        }
    }
    assert_eq!(completed, failed_at);
    completed
}

pub(super) fn exercise(case: &Case, guest: bool) -> u64 {
    let e = Env::new_with_config(EnvTestConfig { capture_snapshot_at_drop: false });
    let f = guest::fixture(&e, guest, 0, 0);
    let id = &f.client.address;
    let currency =
        if case.multi { e.register(MockMultiToken, ()) } else { e.register(MockNonFungible, ()) };
    let mut initial = std::collections::BTreeMap::<u32, i128>::new();
    for &id in &case.ids {
        if let Ok(id) = u32::try_from(id) {
            initial.insert(id, if case.multi { 10 } else { 1 });
        }
    }
    if let Some(&raw) = case.ids.get(case.shortage) {
        if let Ok(id) = u32::try_from(raw) {
            if case.multi {
                if let Some(&quantity) = case.quantities.get(case.shortage).filter(|&&n| n > 0) {
                    let prior: i128 = case
                        .ids
                        .iter()
                        .zip(&case.quantities)
                        .take(case.shortage)
                        .filter(|(other, _)| **other == raw)
                        .map(|(_, quantity)| (*quantity).max(0))
                        .sum();
                    initial.insert(id, prior + quantity - 1);
                }
            } else if !case.ids[..case.shortage].contains(&raw) {
                initial.insert(id, 0);
            }
        }
    }
    e.mock_all_auths();
    for (&token_id, &balance) in &initial {
        if case.multi {
            MockMultiTokenClient::new(&e, &currency).mint(id, &token_id, &balance);
        } else if balance != 0 {
            MockNonFungibleClient::new(&e, &currency).mint(id, &token_id);
        }
    }
    let invalid = case.ids.iter().any(|&id| u32::try_from(id).is_err())
        || case.quantities.iter().any(|&n| n < 0)
        || (case.multi && case.ids.len() != case.quantities.len());
    let mut remaining = initial.clone();
    let mut transferred = std::collections::BTreeMap::<u32, i128>::new();
    let mut failed_at = None;
    if !invalid {
        for (position, &token_id) in case.ids.iter().enumerate() {
            let token_id = token_id as u32;
            let amount = if case.multi { case.quantities[position] } else { 1 };
            let balance = remaining.get_mut(&token_id).unwrap();
            if *balance < amount {
                failed_at = Some(position);
                break;
            }
            *balance -= amount;
            *transferred.entry(token_id).or_default() += amount;
        }
    }
    let token_ids = Vec::from_slice(&e, &case.ids);
    let amounts = Vec::from_slice(&e, &case.quantities);
    let kind = if case.multi { TokenType::MultiToken } else { TokenType::NonFungible };
    let args = (&f.admin, &f.recipient, &currency, kind, 0_i128, &token_ids, &amounts).into_val(&e);
    guest::authorize(&e, id, &f.admin, "remove_funds", args);
    let before = e.to_ledger_snapshot().ledger_entries;
    let actual = f.client.try_remove_funds(
        &f.admin,
        &f.recipient,
        &currency,
        &kind,
        &0,
        &token_ids,
        &amounts,
    );
    let mut observed_prefix = 0;
    if invalid || failed_at.is_some() {
        if invalid {
            assert_eq!(actual, Err(Ok(Error::from_contract_error(6105))));
        } else {
            let failed_at = failed_at.unwrap();
            // OZ: 200 = nonexistent token, 201 = from is not the current owner.
            let nft_error = if initial[&(case.ids[failed_at] as u32)] == 0 { 200 } else { 201 };
            let (error, cause) = if case.multi {
                (
                    Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction),
                    ScError::WasmVm(ScErrorCode::InvalidAction),
                )
            } else {
                (Error::from_contract_error(nft_error), ScError::Contract(nft_error))
            };
            assert_eq!(actual, Err(Ok(error)), "asset failure at {failed_at}: {case:?}");
            observed_prefix = completed_prefix(&e, &f, &currency, case, failed_at, cause);
        }
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        remaining = initial.clone();
        transferred.clear();
    } else {
        assert_eq!(actual, Ok(Ok(())));
        assert_eq!(
            e.events().all().filter_by_contract(id),
            std::vec![FundsRemoved {
                receiver: f.recipient.clone(),
                currency: currency.clone(),
                amount: 0,
                currency_type: kind,
                token_ids,
                amounts,
            }
            .to_xdr(&e, id)]
        );
    }
    for (&token_id, &before) in &initial {
        if case.multi {
            let token = MockMultiTokenClient::new(&e, &currency);
            let paid = transferred.get(&token_id).copied().unwrap_or(0);
            assert_eq!(token.balance(id, &token_id), remaining[&token_id]);
            assert_eq!(token.balance(&f.recipient, &token_id), paid);
            assert_eq!(token.balance(&f.collector, &token_id), 0);
            assert_eq!(remaining[&token_id] + paid, before);
        } else {
            let token = MockNonFungibleClient::new(&e, &currency);
            if before == 0 {
                assert!(token.try_owner_of(&token_id).is_err());
            } else {
                assert_eq!(
                    token.owner_of(&token_id),
                    if remaining[&token_id] == 0 { f.recipient.clone() } else { id.clone() }
                );
            }
        }
    }
    (if case.multi { 1 } else { 2 })
        | if invalid {
            1 << 2
        } else if failed_at.is_some() {
            1 << 3
        } else {
            1 << 4
        }
        | if case.ids.is_empty() { 1 << 5 } else { 0 }
        | if case.ids.iter().enumerate().any(|(i, id)| case.ids[..i].contains(id)) {
            1 << 6
        } else {
            0
        }
        | if observed_prefix > 0 { 1 << 7 } else { 0 }
        | if case.ids.contains(&i128::from(u32::MAX)) { 1 << 8 } else { 0 }
        | if case.ids.contains(&(i128::from(u32::MAX) + 1)) { 1 << 9 } else { 0 }
}
