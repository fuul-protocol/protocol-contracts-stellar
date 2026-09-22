use crate::test::*;
use soroban_sdk::{Error, Val};

#[test]
fn manager_identity_auth_proof_scope_and_late_fee_retry_are_atomic() {
    for compiled in [false, true] {
        let e = Env::default();
        let f = guest::fixture(&e, compiled, 100, 0);
        let id = &f.client.address;
        let p = proof(&e, 91);
        let args: Vec<Val> = (
            &f.manager,
            &f.recipient,
            &f.currency,
            TokenType::StellarAsset,
            1_000_000_i128,
            u(&e, 0),
            &p,
            false,
        )
            .into_val(&e);
        let before = e.to_ledger_snapshot().ledger_entries;
        e.mock_auths(&[]);
        assert!(e
            .try_invoke_contract::<ProjectClaimResult, Error>(
                id,
                &Symbol::new(&e, "claim"),
                args.clone()
            )
            .is_err());
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        guest::authorize(&e, id, &f.manager, "claim", args.clone());
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            e.try_invoke_contract::<ProjectClaimResult, Error>(
                id,
                &Symbol::new(&e, "claim"),
                args.clone()
            ),
            Err(Ok(Error::from_contract_error(10)))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        assert!(!f.client.claimed_proofs(&p));
        guest::authorize(&e, &f.currency, &f.token_admin, "mint", (id, 10_000_i128).into_val(&e));
        StellarAssetClient::new(&e, &f.currency).mint(id, &10_000);
        guest::authorize(&e, id, &f.manager, "claim", args.clone());
        e.invoke_contract::<ProjectClaimResult>(id, &Symbol::new(&e, "claim"), args);
        assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.recipient), 1_000_000);
        assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.collector), 10_000);
        for kind in [TokenType::StellarAsset, TokenType::NonFungible, TokenType::MultiToken] {
            let other = Address::generate(&e);
            let args: Vec<Val> = (
                &f.manager,
                &other,
                &other,
                kind,
                0_i128,
                U256::from_u128(&e, u128::MAX),
                &p,
                false,
            )
                .into_val(&e);
            guest::authorize(&e, id, &f.manager, "claim", args.clone());
            assert_eq!(
                e.try_invoke_contract::<ProjectClaimResult, Error>(
                    id,
                    &Symbol::new(&e, "claim"),
                    args
                ),
                Err(Ok(Error::from_contract_error(6102)))
            );
        }
        for actor in [&f.admin, &f.recipient] {
            let args: Vec<Val> = (
                actor,
                &f.recipient,
                &f.currency,
                TokenType::StellarAsset,
                0_i128,
                u(&e, 0),
                proof(&e, 92),
                false,
            )
                .into_val(&e);
            guest::authorize(&e, id, actor, "claim", args.clone());
            assert_eq!(
                e.try_invoke_contract::<ProjectClaimResult, Error>(
                    id,
                    &Symbol::new(&e, "claim"),
                    args
                ),
                Err(Ok(Error::from_contract_error(6101)))
            );
        }
    }
}

#[test]
fn a_second_project_has_an_independent_proof_namespace() {
    let e = Env::default();
    let a = guest::fixture(&e, false, 0, 0);
    let b = guest::fixture(&e, true, 0, 0);
    let p = proof(&e, 93);
    for f in [&a, &b] {
        let args: Vec<Val> = (
            &f.manager,
            &f.recipient,
            &f.currency,
            TokenType::StellarAsset,
            0_i128,
            u(&e, 0),
            &p,
            false,
        )
            .into_val(&e);
        guest::authorize(&e, &f.client.address, &f.manager, "claim", args.clone());
        e.invoke_contract::<ProjectClaimResult>(&f.client.address, &Symbol::new(&e, "claim"), args);
        assert!(f.client.claimed_proofs(&p));
    }
}
#[test]
fn factory_lookup_failure_after_proof_write_allows_same_proof_retry() {
    for compiled in [false, true] {
        let e = Env::default();
        let f = guest::fixture(&e, compiled, 0, 0);
        let factory = f.client.factory();
        let p = proof(&e, 94);
        let args: Vec<Val> = (
            &f.manager,
            &f.recipient,
            &f.currency,
            TokenType::StellarAsset,
            1_i128,
            u(&e, 0),
            &p,
            false,
        )
            .into_val(&e);
        e.as_contract(&factory, || e.storage().instance().set(&symbol_short!("fail_fee"), &true));
        guest::authorize(&e, &f.client.address, &f.manager, "claim", args.clone());
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            e.try_invoke_contract::<ProjectClaimResult, Error>(
                &f.client.address,
                &Symbol::new(&e, "claim"),
                args.clone()
            ),
            Err(Ok(Error::from_contract_error(1)))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        assert!(!f.client.claimed_proofs(&p));
        e.as_contract(&factory, || e.storage().instance().set(&symbol_short!("fail_fee"), &false));
        guest::authorize(&e, &f.client.address, &f.manager, "claim", args.clone());
        e.invoke_contract::<ProjectClaimResult>(&f.client.address, &Symbol::new(&e, "claim"), args);
        assert!(f.client.claimed_proofs(&p));
    }
}

#[test]
fn token_callback_cannot_reenter_claim_or_consume_its_proof() {
    use soroban_sdk::xdr::{ScErrorCode, ScErrorType};
    for compiled in [false, true] {
        let e = Env::default();
        let f = guest::fixture(&e, compiled, 0, 0);
        let token = e.register(ReentrantProjectToken, (&f.client.address, &f.recipient));
        let p = proof(&e, 95);
        let args: Vec<Val> = (
            &f.manager,
            &f.recipient,
            &token,
            TokenType::StellarAsset,
            1_i128,
            u(&e, 0),
            &p,
            false,
        )
            .into_val(&e);
        guest::authorize(&e, &f.client.address, &f.manager, "claim", args.clone());
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            e.try_invoke_contract::<ProjectClaimResult, Error>(
                &f.client.address,
                &Symbol::new(&e, "claim"),
                args
            ),
            Err(Ok(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        assert!(!f.client.claimed_proofs(&p));
    }
}
