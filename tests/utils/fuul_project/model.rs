use crate::test::*;
use soroban_sdk::{Error, Val};

#[derive(Clone)]
struct Model {
    funds: i128,
    paid: i128,
    fees_paid: i128,
    bps: u32,
    proofs: [bool; 8],
    nft_owned: [bool; 3],
    multi_owned: [i128; 3],
    multi_paid: [i128; 3],
    kyc: bool,
    manager: bool,
    second_admin: bool,
}

// Independent unsigned arithmetic; never calls the production fee implementation.
fn fee(e: &Env, amount: i128, bps: u32) -> i128 {
    U256::from_u128(e, amount as u128)
        .mul(&U256::from_u32(e, bps))
        .div(&U256::from_u32(e, 10_000))
        .to_u128()
        .unwrap() as i128
}

pub fn exercise(actions: &[(u8, u16, u8, bool)], compiled: bool) {
    let e = Env::default();
    e.cost_estimate().budget().reset_unlimited();
    let f = guest::fixture(&e, compiled, 0, 0);
    let id = &f.client.address;
    let factory = f.client.factory();
    let other = Address::generate(&e);
    let b = Address::generate(&e);
    let role = Symbol::new(&e, "default_admin");
    let access = FuulAccessControlClient::new(&e, id);
    let nft = e.register(MockNonFungible, ());
    let multi = e.register(MockMultiToken, ());
    let nft_client = MockNonFungibleClient::new(&e, &nft);
    let multi_client = MockMultiTokenClient::new(&e, &multi);
    for index in 0..3 {
        nft_client.mint(id, &index);
        multi_client.mint(id, &index, &10);
    }
    let mut model = Model {
        funds: 1_000_000,
        paid: 0,
        fees_paid: 0,
        bps: 0,
        proofs: [false; 8],
        nft_owned: [true; 3],
        multi_owned: [10; 3],
        multi_paid: [0; 3],
        kyc: false,
        manager: true,
        second_admin: false,
    };
    for &(op, value, p, flag) in actions {
        let amount = i128::from(value);
        let index = usize::from(p % 3);
        let token_id = index as u32;
        let proof = proof(&e, p);
        let mut next = model.clone();
        match op {
            6 => {
                next.bps = if flag { 10_000 } else { u32::from(value).min(10_000) };
                e.as_contract(&factory, || {
                    e.storage().instance().set(
                        &symbol_short!("fees"),
                        &FeesInformation {
                            fee_collector: f.collector.clone(),
                            fees: ProjectFees {
                                native_user_claim_fee: 0,
                                project_claim_fee: next.bps,
                                remove_fee: next.bps,
                            },
                        },
                    )
                });
            }
            7 => {
                guest::authorize(
                    &e,
                    id,
                    &f.admin,
                    "set_kyc_required",
                    (&f.admin, flag).into_val(&e),
                );
                f.client.set_kyc_required(&f.admin, &flag);
                next.kyc = flag;
            }
            8 => {
                e.as_contract(&factory, || {
                    e.storage()
                        .instance()
                        .set(&symbol_short!("manager"), if flag { &f.manager } else { &other })
                });
                next.manager = flag;
            }
            9 => {
                let name = if flag { "grant_role" } else { "revoke_role" };
                guest::authorize(&e, id, &f.admin, name, (&role, &b, &f.admin).into_val(&e));
                if flag {
                    access.grant_role(&role, &b, &f.admin);
                } else {
                    access.revoke_role(&role, &b, &f.admin);
                }
                next.second_admin = flag;
            }
            _ => {
                let is_claim = op < 3 || op == 10;
                let kind = match op {
                    1 | 4 => TokenType::NonFungible,
                    2 | 5 => TokenType::MultiToken,
                    _ => TokenType::StellarAsset,
                };
                let currency = match kind {
                    TokenType::StellarAsset => &f.currency,
                    TokenType::NonFungible => &nft,
                    TokenType::MultiToken => &multi,
                };
                let self_transfer = flag && kind != TokenType::StellarAsset;
                let to = if self_transfer { id } else { &f.recipient };
                let actor = if is_claim {
                    &f.manager
                } else if flag {
                    &b
                } else {
                    &f.admin
                };
                let mut accepted = if is_claim {
                    model.manager
                        && (!model.kyc || flag)
                        && !model.proofs[usize::from(p)]
                        && op != 10
                } else {
                    !flag || model.second_admin
                };
                let mut ids = Vec::<i128>::new(&e);
                let mut amounts = Vec::<i128>::new(&e);
                if !is_claim && kind != TokenType::StellarAsset && value % 4 != 0 {
                    ids.push_back(i128::from(token_id));
                    amounts.push_back(amount % 15);
                    if value % 4 == 2 {
                        ids.push_back(i128::from(token_id));
                        amounts.push_back(amount % 15);
                    }
                    if kind == TokenType::MultiToken && value % 7 == 0 {
                        amounts.pop_back();
                    }
                }
                if accepted {
                    match kind {
                        TokenType::StellarAsset => {
                            let cost = fee(&e, amount, model.bps);
                            let debit = if is_claim { amount + cost } else { amount };
                            accepted = debit <= model.funds;
                            if accepted {
                                next.funds -= debit;
                                next.paid += if is_claim { amount } else { amount - cost };
                                next.fees_paid += cost;
                            }
                        }
                        TokenType::NonFungible => {
                            let transfers =
                                if is_claim { vec![&e, i128::from(token_id)] } else { ids.clone() };
                            for item in transfers.iter() {
                                if !next.nft_owned[item as usize] {
                                    accepted = false;
                                    break;
                                }
                                if !self_transfer {
                                    next.nft_owned[item as usize] = false;
                                }
                            }
                        }
                        TokenType::MultiToken => {
                            let transfers =
                                if is_claim { vec![&e, i128::from(token_id)] } else { ids.clone() };
                            let quantities =
                                if is_claim { vec![&e, 1_i128] } else { amounts.clone() };
                            accepted = transfers.len() == quantities.len();
                            if accepted {
                                for n in 0..transfers.len() {
                                    let item = transfers.get_unchecked(n) as usize;
                                    let quantity = quantities.get_unchecked(n);
                                    if next.multi_owned[item] < quantity {
                                        accepted = false;
                                        break;
                                    }
                                    if !self_transfer {
                                        next.multi_owned[item] -= quantity;
                                        next.multi_paid[item] += quantity;
                                    }
                                }
                            }
                        }
                    }
                }
                let name = if is_claim { "claim" } else { "remove_funds" };
                let args: Vec<Val> = if is_claim {
                    (actor, to, currency, kind, amount, u(&e, token_id), &proof, flag).into_val(&e)
                } else {
                    (actor, to, currency, kind, amount, &ids, &amounts).into_val(&e)
                };
                if op == 10 {
                    e.mock_auths(&[]);
                } else {
                    guest::authorize(&e, id, actor, name, args.clone());
                }
                let before = e.to_ledger_snapshot().ledger_entries;
                let actual = e.try_invoke_contract::<Val, Error>(id, &Symbol::new(&e, name), args);
                assert_eq!(
                    actual.is_ok(),
                    accepted,
                    "op={op} value={value} proof={p} flag={flag} guest={compiled}"
                );
                if accepted {
                    if is_claim {
                        next.proofs[usize::from(p)] = true;
                    }
                } else {
                    assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
                    assert!(e.events().all().events().is_empty());
                    next = model.clone();
                }
            }
        }
        model = next;
        let token = TokenClient::new(&e, &f.currency);
        assert_eq!(token.balance(id), model.funds);
        assert_eq!(token.balance(&f.recipient), model.paid);
        assert_eq!(token.balance(&f.collector), model.fees_paid);
        assert_eq!(model.funds + model.paid + model.fees_paid, 1_000_000);
        for index in 0..3 {
            assert_eq!(
                nft_client.owner_of(&(index as u32)),
                if model.nft_owned[index] { id.clone() } else { f.recipient.clone() }
            );
            assert_eq!(multi_client.balance(id, &(index as u32)), model.multi_owned[index]);
            assert_eq!(
                multi_client.balance(&f.recipient, &(index as u32)),
                model.multi_paid[index]
            );
            assert_eq!(model.multi_owned[index] + model.multi_paid[index], 10);
        }
        for index in 0..8 {
            assert_eq!(
                f.client.claimed_proofs(&crate::test::proof(&e, index as u8)),
                model.proofs[index]
            );
        }
        assert_eq!(f.client.kyc_required(), model.kyc);
        assert_eq!(access.has_role(&role, &b), model.second_admin);
    }
}
