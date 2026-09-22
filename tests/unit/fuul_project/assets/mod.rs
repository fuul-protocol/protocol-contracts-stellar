use crate::test::*;

#[test]
fn multi_token_self_transfer_preserves_balance_and_guards() {
    use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
    let e = Env::default();
    let id = e.register(MockMultiToken, ());
    let token = MockMultiTokenClient::new(&e, &id);
    let owner = Address::generate(&e);
    token.mint(&owner, &7, &10);
    for amount in [0_i128, 1, 10, 11, -1] {
        e.mock_auths(&[]);
        assert!(token.try_transfer(&owner, &owner, &7, &amount).is_err());
        assert_eq!(token.balance(&owner, &7), 10);
        e.mock_auths(&[MockAuth {
            address: &owner,
            invoke: &MockAuthInvoke {
                contract: &id,
                fn_name: "transfer",
                args: (&owner, &owner, 7_u32, amount).into_val(&e),
                sub_invokes: &[],
            },
        }]);
        assert_eq!(
            token.try_transfer(&owner, &owner, &7, &amount).is_ok(),
            (0..=10).contains(&amount)
        );
        assert_eq!(token.balance(&owner, &7), 10);
    }
    token.mint(&owner, &8, &i128::MAX);
    e.mock_auths(&[MockAuth {
        address: &owner,
        invoke: &MockAuthInvoke {
            contract: &id,
            fn_name: "transfer",
            args: (&owner, &owner, 8_u32, 1_i128).into_val(&e),
            sub_invokes: &[],
        },
    }]);
    token.transfer(&owner, &owner, &8, &1);
    assert_eq!(token.balance(&owner, &8), i128::MAX);
}

#[test]
fn multi_token_id_boundaries_preserve_state_and_allow_same_proof_retry() {
    use soroban_sdk::{Error, Val};
    for compiled in [false, true] {
        let e = Env::default();
        let f = guest::fixture(&e, compiled, 0, 0);
        let token = e.register(MockMultiToken, ());
        let client = MockMultiTokenClient::new(&e, &token);
        client.mint(&f.client.address, &u32::MAX, &2);
        let p = proof(&e, 96);
        for id in [
            u(&e, u64::from(u32::MAX) + 1),
            U256::from_parts(&e, u64::MAX, u64::MAX, u64::MAX, u64::MAX),
        ] {
            let args: Vec<Val> =
                (&f.manager, &f.recipient, &token, TokenType::MultiToken, 0_i128, id, &p, false)
                    .into_val(&e);
            guest::authorize(&e, &f.client.address, &f.manager, "claim", args.clone());
            let before = e.to_ledger_snapshot().ledger_entries;
            assert_eq!(
                e.try_invoke_contract::<ProjectClaimResult, Error>(
                    &f.client.address,
                    &Symbol::new(&e, "claim"),
                    args
                ),
                Err(Ok(Error::from_contract_error(6105)))
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
        }
        let args: Vec<Val> = (
            &f.manager,
            &f.recipient,
            &token,
            TokenType::MultiToken,
            0_i128,
            u(&e, u32::MAX),
            &p,
            false,
        )
            .into_val(&e);
        guest::authorize(&e, &f.client.address, &f.manager, "claim", args.clone());
        e.invoke_contract::<ProjectClaimResult>(&f.client.address, &Symbol::new(&e, "claim"), args);
        assert_eq!(client.balance(&f.recipient, &u32::MAX), 1);
        assert!(f.client.claimed_proofs(&p));
        for ids in [vec![&e, -1_i128], vec![&e, i128::from(u32::MAX) + 1]] {
            let args: Vec<Val> = (
                &f.admin,
                &f.recipient,
                &token,
                TokenType::MultiToken,
                0_i128,
                ids,
                vec![&e, 1_i128],
            )
                .into_val(&e);
            guest::authorize(&e, &f.client.address, &f.admin, "remove_funds", args.clone());
            let before = e.to_ledger_snapshot().ledger_entries;
            assert_eq!(
                e.try_invoke_contract::<(), Error>(
                    &f.client.address,
                    &Symbol::new(&e, "remove_funds"),
                    args
                ),
                Err(Ok(Error::from_contract_error(6105)))
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        }
    }
}
