use super::real_accounts::{account, invocation, setup, signed_entry};
use crate::test::{constructor_helpers::assert_diagnostic, roles_pause_helpers as auth, *};
use ed25519_dalek::SigningKey;
use soroban_sdk::{xdr, Map, TryFromVal};

// Independent EVM field oracle; no production authorization serializer.
pub(super) fn intent(e: &Env, c: &ClaimCheck) -> Map<Symbol, Val> {
    Map::from_array(
        e,
        [
            (Symbol::new(e, "project_address"), c.project_address.clone().into_val(e)),
            (Symbol::new(e, "to"), c.to.clone().into_val(e)),
            (Symbol::new(e, "currency"), c.currency.clone().into_val(e)),
            (Symbol::new(e, "amount"), c.amount.into_val(e)),
            (
                Symbol::new(e, "reason"),
                (Symbol::new(
                    e,
                    match c.reason {
                        ClaimReason::AffiliatePayout => "AffiliatePayout",
                        ClaimReason::EndUserPayout => "EndUserPayout",
                    },
                ),)
                    .into_val(e),
            ),
            (Symbol::new(e, "token_id"), c.token_id.into_val(e)),
            (Symbol::new(e, "deadline"), c.deadline.into_val(e)),
            (Symbol::new(e, "proof"), c.proof.clone().into_val(e)),
        ],
    )
}

fn consent(
    e: &Env,
    manager: &Address,
    caller: &Address,
    key: &SigningKey,
    checks: &Vec<ClaimCheck>,
    nonce: i64,
) -> SorobanAuthorizationEntry {
    signed_entry(
        e,
        caller,
        &[key],
        invocation(manager, (caller, checks).into_val(e)),
        nonce,
        200,
        [5; 32],
    )
}

#[test]
fn authorization_contains_exactly_eight_evm_fields() {
    let e = auth::test_env();
    let signer = Address::generate(&e);
    let (_, checks) = setup(&e, &signer);
    let check = checks.get(0).unwrap();
    let value: Val = check.authorization().into_val(&e);
    let actual = Map::<Symbol, Val>::try_from_val(&e, &value).unwrap();
    assert_eq!(actual, intent(&e, &check));
}

#[test]
fn caller_signer_uses_independent_consent_and_proof_roots() {
    let e = auth::test_env();
    let key = SigningKey::from_bytes(&[81; 32]);
    let signer = account(&e, &key, None, 1);
    let (manager, checks) = setup(&e, &signer);
    let proof = signed_entry(
        &e,
        &signer,
        &[&key],
        invocation(&manager.address, (intent(&e, &checks.get(0).unwrap()),).into_val(&e)),
        401,
        200,
        [5; 32],
    );
    e.set_auths(&[consent(&e, &manager.address, &signer, &key, &checks, 400), proof]);
    assert_eq!(manager.try_claim(&signer, &checks), Ok(Ok(())));
    assert_eq!(manager.users_claims(&signer, &checks.get(0).unwrap().currency), u(&e, 10));
}

#[test]
fn preapproved_claims_allow_caller_change_split_and_reverse_order() {
    for split in [false, true] {
        let e = auth::test_env();
        let key = SigningKey::from_bytes(&[82; 32]);
        let payer_key = SigningKey::from_bytes(&[83; 32]);
        let signer = account(&e, &key, None, 1);
        let payer = account(&e, &payer_key, None, 1);
        let (manager, original) = setup(&e, &signer);
        let first = original.get(0).unwrap();
        let mut second = first.clone();
        second.amount = 20;
        second.proof = BytesN::from_array(&e, &[95; 32]);
        let approvals = [first.clone(), second.clone()].map(|c| {
            signed_entry(
                &e,
                &signer,
                &[&key],
                invocation(&manager.address, (intent(&e, &c),).into_val(&e)),
                c.amount as i64 + 400,
                200,
                [5; 32],
            )
        });
        let batches = if split {
            std::vec![vec![&e, second], vec![&e, first]]
        } else {
            std::vec![vec![&e, second, first]]
        };
        for (i, batch) in batches.iter().enumerate() {
            let mut entries =
                std::vec![consent(&e, &manager.address, &payer, &payer_key, batch, 430 + i as i64)];
            if split {
                entries.push(approvals[1 - i].clone());
            } else {
                entries.extend(approvals.clone());
            }
            e.set_auths(&entries);
            assert_eq!(manager.try_claim(&payer, batch), Ok(Ok(())), "split={split}, batch={i}");
        }
        assert_eq!(manager.users_claims(&signer, &original.get(0).unwrap().currency), u(&e, 30));
    }
}

#[test]
fn asset_adapter_selection_is_caller_consent_not_a_signer_field() {
    let e = auth::test_env();
    let key = SigningKey::from_bytes(&[84; 32]);
    let signer = account(&e, &key, None, 1);
    let (manager, original) = setup(&e, &signer);
    let mut check = original.get(0).unwrap();
    let proof = signed_entry(
        &e,
        &signer,
        &[&key],
        invocation(&manager.address, (intent(&e, &check),).into_val(&e)),
        441,
        200,
        [5; 32],
    );
    check.currency_type = TokenType::NonFungible;
    let checks = vec![&e, check];
    e.set_auths(&[consent(&e, &manager.address, &signer, &key, &checks, 440), proof]);
    assert_eq!(manager.try_claim(&signer, &checks), Ok(Ok(())));
}

#[test]
fn every_signed_field_tamper_rolls_back_before_same_credential_control() {
    for field in
        ["project_address", "to", "currency", "amount", "reason", "token_id", "deadline", "proof"]
    {
        let e = auth::test_env();
        let key = SigningKey::from_bytes(&[85; 32]);
        let signer = account(&e, &key, None, 1);
        let (manager, checks) = setup(&e, &signer);
        let check = checks.get(0).unwrap();
        let good = intent(&e, &check);
        let mut bad = good.clone();
        let replacement = match field {
            "project_address" | "to" | "currency" => Address::generate(&e).into_val(&e),
            "amount" => 11_i128.into_val(&e),
            "token_id" => u(&e, 11).into_val(&e),
            "deadline" => check.deadline.add(&u(&e, 1)).into_val(&e),
            "reason" => (Symbol::new(&e, "EndUserPayout"),).into_val(&e),
            _ => BytesN::from_array(&e, &[96; 32]).into_val(&e),
        };
        bad.set(Symbol::new(&e, field), replacement);
        let caller = consent(&e, &manager.address, &signer, &key, &checks, 450);
        let before = e.to_ledger_snapshot().ledger_entries;
        e.set_auths(&[
            caller.clone(),
            signed_entry(
                &e,
                &signer,
                &[&key],
                invocation(&manager.address, (bad,).into_val(&e)),
                451,
                200,
                [5; 32],
            ),
        ]);
        assert_eq!(
            manager.try_claim(&signer, &checks),
            Err(Ok(auth::native_auth_error())),
            "{field}"
        );
        assert_diagnostic(&e, xdr::ScError::Auth(ScErrorCode::InvalidAction));
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        e.set_auths(&[
            caller,
            signed_entry(
                &e,
                &signer,
                &[&key],
                invocation(&manager.address, (good,).into_val(&e)),
                451,
                200,
                [5; 32],
            ),
        ]);
        assert_eq!(manager.try_claim(&signer, &checks), Ok(Ok(())), "control {field}");
    }
}

#[test]
fn manager_wasm_and_real_sac_separate_proof_and_payment_with_atomic_recovery() {
    use std::rc::Rc;
    for overlap in [false, true] {
        for mutation in 0..8 {
            let e = auth::test_env();
            e.ledger().with_mut(|l| {
                l.timestamp = 1_000_000;
                l.sequence_number = 100;
                l.network_id = [5; 32];
            });
            let signer_key = SigningKey::from_bytes(&[86; 32]);
            let payer_key = SigningKey::from_bytes(&[87; 32]);
            let signer = account(&e, &signer_key, None, 1);
            let payer = if overlap { signer.clone() } else { account(&e, &payer_key, None, 1) };
            let key = if overlap { &signer_key } else { &payer_key };
            for (ledger_key, (entry, ttl)) in e.to_ledger_snapshot().ledger_entries {
                if matches!(ledger_key.as_ref(), xdr::LedgerKey::Account(_)) {
                    let mut funded = (*entry).clone();
                    let xdr::LedgerEntryData::Account(a) = &mut funded.data else { unreachable!() };
                    a.balance = 100_000_000;
                    e.host()
                        .add_ledger_entry(&Rc::new(*ledger_key), &Rc::new(funded), ttl)
                        .unwrap();
                }
            }
            let admin = Address::generate(&e);
            let currency = e.register_stellar_asset_contract_v2(admin.clone()).address();
            // XDR Asset::Native: real XLM SAC, backed by the seeded G-account balance.
            let native = e
                .deployer()
                .with_stellar_asset(soroban_sdk::Bytes::from_array(&e, &[0; 4]))
                .deploy();
            let collector = Address::generate(&e);
            let id = e.register(
                constructor_helpers::MANAGER_WASM,
                (
                    &admin,
                    &admin,
                    &admin,
                    1_u128,
                    vec![&e, signer.clone()],
                    &currency,
                    &native,
                    None::<Address>,
                    u(&e, 1_000_000_000_000_i128),
                ),
            );
            let manager = FuulManagerClient::new(&e, &id);
            let factory = e.register(
                FuulFactory,
                (&admin, &id, &collector, e.deployer().upload_contract_wasm(PROJECT_WASM)),
            );
            e.mock_all_auths();
            let project = FuulFactoryClient::new(&e, &factory).create_fuul_project(
                &admin,
                &String::from_str(&e, "ipfs://per-claim"),
                &false,
            );
            StellarAssetClient::new(&e, &currency).mint(&project, &202_000);
            let mut checks = Vec::new(&e);
            for n in [111_u8, 112] {
                checks.push_back(ClaimCheck {
                    project_address: project.clone(),
                    to: collector.clone(),
                    currency: currency.clone(),
                    currency_type: TokenType::StellarAsset,
                    amount: 100_000,
                    reason: ClaimReason::AffiliatePayout,
                    token_id: if n == 111 {
                        U256::from_parts(&e, u64::MAX, u64::MAX, u64::MAX, u64::MAX)
                    } else {
                        u(&e, 0)
                    },
                    deadline: u(&e, 1_000_300),
                    proof: BytesN::from_array(&e, &[n; 32]),
                    signers: vec![&e, signer.clone()],
                });
            }
            let payment_root = |asset: &Address, from: &Address, to: &Address, amount: i128| {
                let mut root =
                    invocation(asset, (from, MuxedAddress::from(to), amount).into_val(&e));
                let xdr::SorobanAuthorizedFunction::ContractFn(f) = &mut root.function else {
                    unreachable!()
                };
                f.function_name = "transfer".try_into().unwrap();
                root
            };
            let mut caller_root = invocation(&id, (&payer, &checks).into_val(&e));
            caller_root.sub_invocations =
                std::vec![payment_root(&native, &payer, &collector, 40_000)].try_into().unwrap();
            let caller_entry =
                |root, nonce| signed_entry(&e, &payer, &[key], root, nonce, 200, [5; 32]);
            let proofs: std::vec::Vec<_> = checks
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    signed_entry(
                        &e,
                        &signer,
                        &[&signer_key],
                        invocation(&id, (intent(&e, &c),).into_val(&e)),
                        511 + i as i64,
                        200,
                        [5; 32],
                    )
                })
                .collect();
            let mut valid = std::vec![caller_entry(caller_root.clone(), 510)];
            valid.extend(proofs.clone());
            let mut bad = valid.clone();
            if mutation < 5 {
                let mut root = caller_root.clone();
                root.sub_invocations = match mutation {
                    0 => std::vec![],
                    1 => std::vec![payment_root(&currency, &payer, &collector, 40_000)],
                    2 => std::vec![payment_root(&native, &payer, &admin, 40_000)],
                    3 => std::vec![payment_root(&native, &payer, &collector, 39_999)],
                    _ => std::vec![payment_root(&native, &admin, &collector, 40_000)],
                }
                .try_into()
                .unwrap();
                bad[0] = caller_entry(root, 510);
            } else if mutation == 5 {
                bad.pop();
            } else if mutation == 6 {
                bad[2] = signed_entry(
                    &e,
                    &signer,
                    &[&signer_key],
                    proofs[1].root_invocation.clone(),
                    511,
                    200,
                    [5; 32],
                );
            } else {
                bad.remove(0);
            }
            let before = e.to_ledger_snapshot().ledger_entries;
            e.set_auths(&bad);
            assert_eq!(
                manager.try_claim(&payer, &checks),
                Err(Ok(auth::native_auth_error())),
                "overlap={overlap}, mutation={mutation}"
            );
            assert_diagnostic(
                &e,
                xdr::ScError::Auth(if mutation == 6 {
                    ScErrorCode::ExistingValue
                } else {
                    ScErrorCode::InvalidAction
                }),
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
            e.set_auths(&valid);
            manager.claim(&payer, &checks);
            assert_eq!(e.events().all().filter_by_contract(&native).events().len(), 1);
            assert_eq!(TokenClient::new(&e, &native).balance(&payer), 99_960_000);
            assert_eq!(TokenClient::new(&e, &native).balance(&collector), 40_000);
            assert_eq!(TokenClient::new(&e, &currency).balance(&project), 0);
            assert_eq!(TokenClient::new(&e, &currency).balance(&collector), 202_000);
            assert_eq!(manager.users_claims(&collector, &currency), u(&e, 200_000));
            for check in checks.iter() {
                assert!(FuulProjectClient::new(&e, &project).claimed_proofs(&check.proof));
            }
            let before_replay = e.to_ledger_snapshot().ledger_entries;
            valid[0] = caller_entry(caller_root.clone(), 520);
            e.set_auths(&valid);
            assert_eq!(manager.try_claim(&payer, &checks), Err(Ok(auth::native_auth_error())));
            assert_diagnostic(&e, xdr::ScError::Auth(ScErrorCode::ExistingValue));
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before_replay);
            for (i, entry) in valid.iter_mut().enumerate().skip(1) {
                *entry = signed_entry(
                    &e,
                    &signer,
                    &[&signer_key],
                    entry.root_invocation.clone(),
                    530 + i as i64,
                    200,
                    [5; 32],
                );
            }
            e.set_auths(&valid);
            assert_eq!(
                manager.try_claim(&payer, &checks),
                Err(Ok(Error::from_contract_error(6102)))
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before_replay);
            assert!(e.events().all().events().is_empty());
        }
    }
}
