use crate::test::{constructor_helpers::*, roles_pause_helpers::test_env, *};

#[test]
fn source_configuration_values_and_authenticated_outsiders() {
    for compiled in [false, true] {
        let e = test_env();
        let mut input = Bootstrap::new(&e);
        input.accepted = e.register_stellar_asset_contract_v2(input.admin.clone()).address();
        assert_eq!(TokenClient::new(&e, &input.accepted).decimals(), 7);
        let id =
            if compiled { e.register(MANAGER_WASM, input.args(&e)) } else { input.register(&e) };
        let c = FuulManagerClient::new(&e, &id);
        let outsider = Address::generate(&e);
        let missing = Address::generate(&e);
        let exempt = Address::generate(&e);
        e.mock_all_auths();
        c.add_no_claim_fee_address(&input.admin, &exempt);
        // Source 100-token operations and 2,000-token reduction use the actual seven-decimal asset.
        let hundred = u(&e, 1_000_000_000_i128);
        let cases: [(&str, Vec<Val>); 5] = [
            ("set_claim_cooldown", (&outsider, 1_000_u128).into_val(&e)),
            ("add_currency_limit", (&outsider, &missing, &hundred).into_val(&e)),
            ("set_currency_token_limit", (&outsider, &input.accepted, &hundred).into_val(&e)),
            ("add_no_claim_fee_address", (&outsider, &missing).into_val(&e)),
            ("remove_no_claim_fee_address", (&outsider, &exempt).into_val(&e)),
        ];
        for (name, args) in cases {
            e.mock_auths(&[MockAuth {
                address: &outsider,
                invoke: &MockAuthInvoke {
                    contract: &id,
                    fn_name: name,
                    args: args.clone(),
                    sub_invokes: &[],
                },
            }]);
            let before = e.to_ledger_snapshot().ledger_entries;
            assert_eq!(
                e.try_invoke_contract::<(), Error>(&id, &Symbol::new(&e, name), args),
                Err(Ok(Error::from_contract_error(2000))),
                "{name}"
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
        }
        e.mock_all_auths();
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            c.try_set_currency_token_limit(&input.admin, &missing, &hundred),
            Err(Ok(Error::from_contract_error(6300)))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        let previous = c.currency_limits(&input.accepted);
        let limit = u(&e, 20_000_000_000_i128);
        assert!(previous.claim_limit_per_cooldown > limit);
        c.set_currency_token_limit(&input.admin, &input.accepted, &limit);
        assert_eq!(
            e.events().all().filter_by_contract(&id),
            std::vec![TokenLimitUpdated { token: input.accepted.clone(), limit: limit.clone() }
                .to_xdr(&e, &id)]
        );
        assert_eq!(
            c.currency_limits(&input.accepted),
            CurrencyTokenLimit { claim_limit_per_cooldown: limit, ..previous }
        );
    }
}
