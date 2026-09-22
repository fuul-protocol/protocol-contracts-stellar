use crate::test::*;
use soroban_sdk::{
    testutils::EnvTestConfig,
    xdr::{ScErrorCode, ScErrorType},
    Error, Val,
};

#[derive(Clone, Copy, Debug)]
pub(super) struct Case {
    pub scenario: u8,
    pub scope: u8,
    pub amount: u16,
    pub proof: [u8; 32],
}

pub(super) fn exercise(case: Case, guest: bool) -> u64 {
    let e = Env::new_with_config(EnvTestConfig { capture_snapshot_at_drop: false });
    let f = configured_project_fixture(
        &e,
        guest,
        case.scenario == 5,
        ProjectFees { native_user_claim_fee: 0, project_claim_fee: 0, remove_fee: 0 },
        1_000_000,
    );
    let id = &f.client.address;
    let p = BytesN::from_array(&e, &case.proof);
    let amount = i128::from(case.amount);
    if case.scenario == 6 {
        let role = Symbol::new(&e, "default_admin");
        let access = FuulAccessControlClient::new(&e, id);
        guest::authorize(
            &e,
            id,
            &f.admin,
            "grant_role",
            (&role, &f.recipient, &f.admin).into_val(&e),
        );
        access.grant_role(&role, &f.recipient, &f.admin);
        guest::authorize(
            &e,
            id,
            &f.recipient,
            "revoke_role",
            (&role, &f.admin, &f.recipient).into_val(&e),
        );
        access.revoke_role(&role, &f.admin, &f.recipient);
        let args: Vec<Val> = (
            &f.admin,
            &f.recipient,
            &f.currency,
            TokenType::StellarAsset,
            amount,
            Vec::<i128>::new(&e),
            Vec::<i128>::new(&e),
        )
            .into_val(&e);
        guest::authorize(&e, id, &f.admin, "remove_funds", args.clone());
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            e.try_invoke_contract::<(), Error>(id, &Symbol::new(&e, "remove_funds"), args),
            Err(Ok(Error::from_contract_error(2000)))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        assert!(!access.has_role(&role, &f.admin));
        assert!(access.has_role(&role, &f.recipient));
        return 1 << 6;
    }
    let actor = if case.scenario == 3 { &f.recipient } else { &f.manager };
    let args: Vec<Val> =
        (actor, &f.recipient, &f.currency, TokenType::StellarAsset, amount, u(&e, 0), &p, false)
            .into_val(&e);
    let factory = f.client.factory();
    if case.scenario == 4 {
        // Finish fixture mutation before installing the exact auth for the tested invocation.
        e.as_contract(&factory, || e.storage().instance().set(&symbol_short!("manager"), &f.admin));
        assert!(!MockFactoryClient::new(&e, &factory).has_manager_role(&f.manager));
    }
    e.mock_auths(&[]);
    if case.scenario != 1 {
        guest::authorize(
            &e,
            if case.scenario == 2 { &f.currency } else { id },
            actor,
            "claim",
            args.clone(),
        );
    }
    let before = e.to_ledger_snapshot().ledger_entries;
    let actual =
        e.try_invoke_contract::<ProjectClaimResult, Error>(id, &symbol_short!("claim"), args);
    let expected_error = match case.scenario {
        0 => None,
        1 | 2 => Some(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)),
        3 | 4 => Some(Error::from_contract_error(6101)),
        5 => Some(Error::from_contract_error(6103)),
        _ => unreachable!(),
    };
    if let Some(error) = expected_error {
        assert_eq!(actual, Err(Ok(error)), "{case:?}");
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        assert!(!f.client.claimed_proofs(&p));
        if case.scenario == 4 {
            e.as_contract(&factory, || {
                e.storage().instance().set(&symbol_short!("manager"), &f.manager)
            });
        }
        let args = (
            &f.manager,
            &f.recipient,
            &f.currency,
            TokenType::StellarAsset,
            amount,
            u(&e, 0),
            &p,
            true,
        )
            .into_val(&e);
        guest::authorize(&e, id, &f.manager, "claim", args);
        assert_eq!(
            f.client.claim(
                &f.manager,
                &f.recipient,
                &f.currency,
                &TokenType::StellarAsset,
                &amount,
                &u(&e, 0),
                &p,
                &true
            ),
            ProjectClaimResult { native_user_claim_fee: 0, fee_collector: f.collector.clone() }
        );
    } else {
        assert_eq!(
            actual,
            Ok(Ok(ProjectClaimResult {
                native_user_claim_fee: 0,
                fee_collector: f.collector.clone()
            }))
        );
    }
    assert!(f.client.claimed_proofs(&p));
    assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.recipient), amount);
    e.mock_all_auths();
    let (currency, kind) = match case.scope {
        0 => (f.currency.clone(), TokenType::StellarAsset),
        1 => {
            let currency = e.register_stellar_asset_contract_v2(f.token_admin.clone()).address();
            StellarAssetClient::new(&e, &currency).mint(id, &amount);
            (currency, TokenType::StellarAsset)
        }
        2 => {
            let currency = e.register(MockNonFungible, ());
            MockNonFungibleClient::new(&e, &currency).mint(id, &7);
            (currency, TokenType::NonFungible)
        }
        3 => {
            let currency = e.register(MockMultiToken, ());
            MockMultiTokenClient::new(&e, &currency).mint(id, &7, &2);
            (currency, TokenType::MultiToken)
        }
        _ => unreachable!(),
    };
    if case.scope != 0 {
        assert_ne!(currency, f.currency, "cross-currency replay requires a distinct asset");
    }
    let other = Address::generate(&e);
    let args: Vec<Val> =
        (&f.manager, &other, &currency, kind, amount, u(&e, 7), &p, false).into_val(&e);
    guest::authorize(&e, id, &f.manager, "claim", args.clone());
    let before = e.to_ledger_snapshot().ledger_entries;
    assert_eq!(
        e.try_invoke_contract::<ProjectClaimResult, Error>(id, &symbol_short!("claim"), args),
        Err(Ok(Error::from_contract_error(6102)))
    );
    assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
    assert!(e.events().all().events().is_empty());
    let second = configured_project_fixture(
        &e,
        guest,
        false,
        ProjectFees { native_user_claim_fee: 0, project_claim_fee: 0, remove_fee: 0 },
        0,
    );
    StellarAssetClient::new(&e, &f.currency).mint(&second.client.address, &amount);
    assert!(!second.client.claimed_proofs(&p));
    let args = (
        &second.manager,
        &other,
        &f.currency,
        TokenType::StellarAsset,
        amount,
        u(&e, 0),
        &p,
        false,
    )
        .into_val(&e);
    guest::authorize(&e, &second.client.address, &second.manager, "claim", args);
    assert_eq!(
        second.client.claim(
            &second.manager,
            &other,
            &f.currency,
            &TokenType::StellarAsset,
            &amount,
            &u(&e, 0),
            &p,
            &false
        ),
        ProjectClaimResult { native_user_claim_fee: 0, fee_collector: second.collector }
    );
    assert!(second.client.claimed_proofs(&p));
    assert!(f.client.claimed_proofs(&p));
    assert_eq!(TokenClient::new(&e, &f.currency).balance(&other), amount);
    (1 << case.scenario) | (1 << (8 + case.scope))
}
