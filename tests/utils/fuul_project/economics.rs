use crate::test::*;
use soroban_sdk::{
    testutils::EnvTestConfig,
    xdr::{ContractEventBody, ScVal},
    Error, Map, TryFromVal, Val,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct Case {
    pub remove: bool,
    pub amount: i128,
    pub claim_bps: u32,
    pub remove_bps: u32,
    pub quote: i128,
    pub aliases: u8,
    pub funding_delta: i8,
}

pub(super) fn exercise(case: Case, guest: bool) -> u64 {
    let e = Env::new_with_config(EnvTestConfig { capture_snapshot_at_drop: false });
    let amount = case.amount.max(0);
    let bps = if case.remove { case.remove_bps } else { case.claim_bps };
    let fee = U256::from_u128(&e, amount as u128)
        .mul(&U256::from_u32(&e, bps))
        .div(&U256::from_u32(&e, 10_000))
        .to_u128()
        .unwrap() as i128;
    let (recipient, collector) =
        [(1, 2), (0, 2), (1, 0), (1, 1), (0, 0)][usize::from(case.aliases)];
    let first = if case.remove { amount - fee } else { amount };
    // SAC checks the sender balance even for a self-transfer.
    let required =
        (first as u128).max(if recipient == 0 { fee as u128 } else { first as u128 + fee as u128 });
    let funding = match case.funding_delta {
        -1 => required.saturating_sub(1),
        0 => required,
        1 => required + 1,
        _ => unreachable!(),
    }
    .min(i128::MAX as u128) as i128;
    let f = configured_project_fixture(
        &e,
        guest,
        false,
        ProjectFees {
            native_user_claim_fee: case.quote,
            project_claim_fee: case.claim_bps,
            remove_fee: case.remove_bps,
        },
        funding,
    );
    let id = &f.client.address;
    let addresses = [id.clone(), f.recipient.clone(), f.collector.clone()];
    let factory = f.client.factory();
    e.as_contract(&factory, || {
        e.storage().instance().set(
            &symbol_short!("fees"),
            &FeesInformation {
                fee_collector: addresses[collector].clone(),
                fees: ProjectFees {
                    native_user_claim_fee: case.quote,
                    project_claim_fee: case.claim_bps,
                    remove_fee: case.remove_bps,
                },
            },
        )
    });
    let mut balances = [funding, 0, 0];
    let mut error = (case.amount < 0).then_some(6105);
    let mut late = false;
    for (position, (to, value)) in [(recipient, first), (collector, fee)].into_iter().enumerate() {
        if error.is_some() {
            break;
        }
        if balances[0] < value {
            error = Some(10);
            late = position == 1;
            break;
        }
        if to != 0 {
            balances[0] -= value;
            balances[to] += value;
        }
    }
    let p = proof(&e, 121);
    let (method, actor, args): (&str, &Address, Vec<Val>) = if case.remove {
        (
            "remove_funds",
            &f.admin,
            (
                &f.admin,
                &addresses[recipient],
                &f.currency,
                TokenType::StellarAsset,
                case.amount,
                Vec::<i128>::new(&e),
                Vec::<i128>::new(&e),
            )
                .into_val(&e),
        )
    } else {
        (
            "claim",
            &f.manager,
            (
                &f.manager,
                &addresses[recipient],
                &f.currency,
                TokenType::StellarAsset,
                case.amount,
                u(&e, 0),
                &p,
                false,
            )
                .into_val(&e),
        )
    };
    guest::authorize(&e, id, actor, method, args.clone());
    let before = e.to_ledger_snapshot().ledger_entries;
    let actual = e.try_invoke_contract::<Val, Error>(id, &Symbol::new(&e, method), args);
    if let Some(code) = error {
        assert_eq!(actual.err(), Some(Ok(Error::from_contract_error(code))), "{case:?}");
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        balances = [funding, 0, 0];
    } else {
        let value = actual.unwrap().unwrap();
        if case.remove {
            let events = e.events().all().filter_by_contract(id);
            assert_eq!(events.events().len(), 1);
            let ContractEventBody::V0(body) = &events.events()[0].body;
            assert_eq!(
                body.topics.as_slice(),
                &[ScVal::Symbol("funds_removed".try_into().unwrap())]
            );
            let expected: Map<Symbol, Val> = Map::from_array(
                &e,
                [
                    (Symbol::new(&e, "receiver"), addresses[recipient].clone().into_val(&e)),
                    (Symbol::new(&e, "currency"), f.currency.clone().into_val(&e)),
                    (Symbol::new(&e, "amount"), case.amount.into_val(&e)),
                    (
                        Symbol::new(&e, "currency_type"),
                        (Symbol::new(&e, "StellarAsset"),).into_val(&e),
                    ),
                    (Symbol::new(&e, "token_ids"), Vec::<i128>::new(&e).into_val(&e)),
                    (Symbol::new(&e, "amounts"), Vec::<i128>::new(&e).into_val(&e)),
                ],
            );
            assert_eq!(body.data, ScVal::try_from_val(&e, &expected).unwrap());
        } else {
            assert_eq!(
                ProjectClaimResult::try_from_val(&e, &value).unwrap(),
                ProjectClaimResult {
                    native_user_claim_fee: case.quote,
                    fee_collector: addresses[collector].clone(),
                }
            );
            assert!(e.events().all().filter_by_contract(id).events().is_empty());
        }
    }
    let token = TokenClient::new(&e, &f.currency);
    for (address, expected) in addresses.iter().zip(balances) {
        assert_eq!(token.balance(address), expected, "{case:?}");
    }
    assert_eq!(balances.iter().sum::<i128>(), funding);
    assert_eq!(f.client.claimed_proofs(&p), !case.remove && error.is_none());
    (1 << case.aliases)
        | if case.remove { 1 << 5 } else { 1 << 6 }
        | if error.is_none() {
            1 << 7
        } else if error == Some(10) {
            1 << 8
        } else {
            1 << 9
        }
        | if amount > i128::from(u64::MAX) { 1 << 10 } else { 0 }
        | if late { 1 << 11 } else { 0 }
}
