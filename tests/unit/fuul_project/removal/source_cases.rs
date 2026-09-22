use crate::test::*;
use soroban_sdk::Error;

#[test]
fn source_repeated_different_amounts_preserve_percentage_fees() {
    for compiled in [false, true] {
        // Both assets use seven decimals: source human amounts 5/3 and 200/300.
        for (bps, amounts, fees) in [
            (1_000, [50_000_000_i128, 30_000_000], [5_000_000_i128, 3_000_000]),
            (500, [2_000_000_000_i128, 3_000_000_000], [100_000_000_i128, 150_000_000]),
        ] {
            let e = Env::default();
            let f = guest::fixture(&e, compiled, 0, bps);
            e.mock_all_auths();
            let token = TokenClient::new(&e, &f.currency);
            assert_eq!(token.decimals(), 7);
            let total = amounts.iter().sum::<i128>();
            StellarAssetClient::new(&e, &f.currency).mint(&f.client.address, &total);
            let mut gross = 0;
            let mut paid_fees = 0;
            for (amount, fee) in amounts.into_iter().zip(fees) {
                f.client.remove_funds(
                    &f.admin,
                    &f.recipient,
                    &f.currency,
                    &TokenType::StellarAsset,
                    &amount,
                    &Vec::new(&e),
                    &Vec::new(&e),
                );
                gross += amount;
                paid_fees += fee;
                assert_eq!(token.balance(&f.recipient), gross - paid_fees);
                assert_eq!(token.balance(&f.collector), paid_fees);
                assert_eq!(token.balance(&f.client.address), 1_000_000 + total - gross);
            }
        }
    }
}

#[test]
fn source_small_amount_at_333_bps_has_a_nonzero_fee() {
    for compiled in [false, true] {
        let e = Env::default();
        let f = guest::fixture(&e, compiled, 0, 333);
        e.mock_all_auths();
        let token = TokenClient::new(&e, &f.currency);
        assert_eq!(token.decimals(), 7);
        // 10^15 source units / 10^18 = 0.001 token = 10,000 seven-decimal units.
        f.client.remove_funds(
            &f.admin,
            &f.recipient,
            &f.currency,
            &TokenType::StellarAsset,
            &10_000,
            &Vec::new(&e),
            &Vec::new(&e),
        );
        assert_eq!(token.balance(&f.recipient), 9_667);
        assert_eq!(token.balance(&f.collector), 333);
        assert_eq!(token.balance(&f.client.address), 990_000);
    }
}

#[test]
fn source_multi_token_removal_at_ten_percent_charges_no_token_fee() {
    for compiled in [false, true] {
        let e = Env::default();
        let f = guest::fixture(&e, compiled, 0, 1_000);
        e.mock_all_auths();
        let id = e.register(MockMultiToken, ());
        let token = MockMultiTokenClient::new(&e, &id);
        token.mint(&f.client.address, &1, &100);
        f.client.remove_funds(
            &f.admin,
            &f.recipient,
            &id,
            &TokenType::MultiToken,
            &0,
            &vec![&e, 1_i128],
            &vec![&e, 100_i128],
        );
        assert_eq!(token.balance(&f.client.address, &1), 0);
        assert_eq!(token.balance(&f.recipient, &1), 100);
        assert_eq!(token.balance(&f.collector, &1), 0);
    }
}

#[contract]
struct FailingNft;
#[contractimpl]
impl FailingNft {
    pub fn transfer(e: Env, _from: Address, _to: Address, _id: u32) {
        e.panic_with_error(Error::from_contract_error(2));
    }
}
#[contract]
struct FailingMulti;
#[contractimpl]
impl FailingMulti {
    pub fn transfer(e: Env, _from: Address, _to: Address, _id: u32, _amount: i128) {
        e.panic_with_error(Error::from_contract_error(2));
    }
}

#[test]
fn factory_lookup_precedes_a_distinguishable_failing_asset_transfer() {
    for compiled in [false, true] {
        for kind in [TokenType::NonFungible, TokenType::MultiToken] {
            let e = Env::default();
            let f = guest::fixture(&e, compiled, 0, 0);
            let factory = f.client.factory();
            let token = if kind == TokenType::NonFungible {
                e.register(FailingNft, ())
            } else {
                e.register(FailingMulti, ())
            };
            e.mock_all_auths();
            for (fail_lookup, expected) in [(true, 1), (false, 2)] {
                e.as_contract(&factory, || {
                    e.storage().instance().set(&symbol_short!("fail_fee"), &fail_lookup)
                });
                let before = e.to_ledger_snapshot().ledger_entries;
                assert_eq!(
                    f.client.try_remove_funds(
                        &f.admin,
                        &f.recipient,
                        &token,
                        &kind,
                        &0,
                        &vec![&e, 1_i128],
                        &vec![&e, 1_i128]
                    ),
                    Err(Ok(Error::from_contract_error(expected)))
                );
                assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
                assert!(e.events().all().events().is_empty());
            }
        }
    }
}
