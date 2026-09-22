use crate::test::{roles_pause_helpers::test_env, source_settlement_helpers::*, *};

const UNIT: i128 = 10_000_000;

#[test]
fn source_quorum_cases_pay_real_recipients_and_emit_exact_events() {
    for compiled in [false, true] {
        for (required, provided) in [(2_u128, 2_u32), (3, 3), (2, 3)] {
            let e = test_env();
            let f = setup(&e, compiled);
            fees(&f, 0, 0, 0);
            f.manager.set_required_signers(&f.admin, &required);
            assert_eq!(f.manager.required_signers(), required);
            let mut c = check(&e, &f, 0, &f.currency, 100 * UNIT, 1);
            c.signers = f.signers.slice(0..provided);
            fund(&e, &f, &f.currency, &f.projects[0], c.amount);
            let checks = vec![&e, c];
            security_helpers::authorize_claims(&e, &f.manager.address, &f.payer, &checks, &[]);
            f.manager.claim(&f.payer, &checks);
            assert_claim_events(&e, &f, &checks);
            assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.recipient), 100 * UNIT);
            assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.projects[0]), 0);
            assert_eq!(f.manager.users_claims(&f.recipient, &f.currency), u(&e, 100 * UNIT));
        }
    }
}

#[test]
fn source_signer_rejections_preserve_exact_quorum_and_asset_contexts() {
    for compiled in [false, true] {
        for scenario in 0..6 {
            let e = test_env();
            let f = setup(&e, compiled);
            fees(&f, 0, 0, 0);
            let mut c = check(&e, &f, 0, &f.currency, 100 * UNIT, 2);
            let outsider = Address::generate(&e);
            let expected = match scenario {
                0 => {
                    f.manager.set_required_signers(&f.admin, &2);
                    c.signers.push_back(outsider);
                    6307
                }
                1 => {
                    f.manager.set_required_signers(&f.admin, &2);
                    c.signers.push_back(c.signers.get(0).unwrap());
                    6301
                }
                2 => {
                    f.manager.set_required_signers(&f.admin, &3);
                    c.signers = f.signers.slice(0..2);
                    6306
                }
                3 | 4 => {
                    c.amount = 0;
                    c.token_id = u(&e, 1);
                    c.currency_type =
                        if scenario == 3 { TokenType::NonFungible } else { TokenType::MultiToken };
                    c.currency = if scenario == 3 {
                        e.register(MockNonFungible, ())
                    } else {
                        e.register(MockMultiToken, ())
                    };
                    c.signers = vec![&e, outsider];
                    6307
                }
                _ => {
                    c.currency = f.native.clone();
                    c.amount = 5 * UNIT;
                    c.signers = vec![&e, outsider];
                    6307
                }
            };
            let checks = vec![&e, c.clone()];
            security_helpers::authorize_claims(&e, &f.manager.address, &f.payer, &checks, &[]);
            let before = e.to_ledger_snapshot().ledger_entries;
            assert_eq!(
                f.manager.try_claim(&f.payer, &checks),
                Err(Ok(Error::from_contract_error(expected)))
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
            assert!(!FuulProjectClient::new(&e, &f.projects[0]).claimed_proofs(&c.proof));
        }
    }
}

#[test]
fn source_sequential_claims_accumulate_in_the_same_bucket() {
    for compiled in [false, true] {
        let e = test_env();
        let f = setup(&e, compiled);
        fees(&f, 0, 0, 0);
        assert_eq!(f.manager.users_claims(&f.recipient, &f.native), u(&e, 0));
        fund(&e, &f, &f.currency, &f.projects[0], 300 * UNIT);
        let mut total = 0;
        for (n, amount) in [50, 100, 150].into_iter().enumerate() {
            let c = check(&e, &f, 0, &f.currency, amount * UNIT, n as u8 + 10);
            f.manager.claim(&f.payer, &vec![&e, c]);
            total += amount * UNIT;
            assert_eq!(f.manager.users_claims(&f.recipient, &f.currency), u(&e, total));
            assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.recipient), total);
        }
    }
}

#[test]
fn source_two_recipient_batches_settle_both_balances_and_deadlines_atomically() {
    for compiled in [false, true] {
        for amounts in [[100 * UNIT, 100 * UNIT], [30 * UNIT, 40 * UNIT]] {
            let e = test_env();
            let f = setup(&e, compiled);
            fees(&f, 0, 0, 0);
            assert_eq!(f.manager.kyc_validator(), None);
            let first = check(&e, &f, 0, &f.currency, amounts[0], 20);
            let mut second = check(&e, &f, 0, &f.currency, amounts[1], 21);
            second.to = Address::generate(&e);
            second.deadline = u(&e, e.ledger().timestamp() + 7_200);
            fund(&e, &f, &f.currency, &f.projects[0], amounts.iter().sum());
            let mut expired = second.clone();
            expired.deadline = u(&e, e.ledger().timestamp() - 1);
            let before = e.to_ledger_snapshot().ledger_entries;
            assert_eq!(
                f.manager.try_claim(&f.payer, &vec![&e, first.clone(), expired]),
                Err(Ok(Error::from_contract_error(6305)))
            );
            assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
            assert!(e.events().all().events().is_empty());
            let checks = vec![&e, first.clone(), second.clone()];
            f.manager.claim(&f.payer, &checks);
            assert_claim_events(&e, &f, &checks);
            let token = TokenClient::new(&e, &f.currency);
            assert_eq!(token.balance(&first.to), amounts[0]);
            assert_eq!(token.balance(&second.to), amounts[1]);
            assert_eq!(token.balance(&f.projects[0]), 0);
            for c in checks {
                assert!(FuulProjectClient::new(&e, &c.project_address).claimed_proofs(&c.proof));
                assert_eq!(f.manager.users_claims(&c.to, &c.currency), u(&e, c.amount));
            }
        }
    }
}

#[test]
fn source_native_payouts_project_fees_and_user_fees_settle_exactly() {
    for compiled in [false, true] {
        // (BPS per Project, user fees in stroops, claim count, same Project, exemption, coins).
        for (bps, user_fees, count, same, exempt, coins) in [
            ([0, 0], [0_i128, 0], 2, false, false, 3),
            ([0, 0], [100_000, 200_000], 2, false, false, 3),
            ([0, 0], [100_000, 0], 2, true, false, 3),
            ([0, 0], [100_000, 200_000], 2, false, true, 3),
            ([200, 0], [0, 0], 1, false, false, 5),
            ([100, 200], [0, 0], 2, false, false, 5),
            ([200, 0], [50_000, 0], 1, false, false, 5),
        ] {
            let e = test_env();
            let f = setup(&e, compiled);
            for p in 0..2 {
                fees(&f, p, bps[p], user_fees[p]);
            }
            if exempt {
                f.manager.add_no_claim_fee_address(&f.admin, &f.payer);
            }
            let native = TokenClient::new(&e, &f.native);
            let initial = native.balance(&f.payer);
            let mut checks = Vec::new(&e);
            let mut project_fees = 0;
            let mut user_fee_total = 0;
            for n in 0..count {
                let p = if same { 0 } else { n };
                let amount = coins * UNIT;
                let project_fee = fee(&e, amount, bps[p]);
                fund(&e, &f, &f.native, &f.projects[p], amount + project_fee);
                let mut c = check(&e, &f, p, &f.native, amount, n as u8 + 30);
                c.to = f.payer.clone();
                if n == 1 {
                    c.reason = ClaimReason::EndUserPayout;
                }
                checks.push_back(c);
                project_fees += project_fee;
                if !exempt {
                    user_fee_total += user_fees[p];
                }
            }
            f.manager.claim(&f.payer, &checks);
            assert_claim_events(&e, &f, &checks);
            // Unit VM calls have no network transaction fee; payer and payout recipient coincide.
            assert_eq!(
                native.balance(&f.payer),
                initial + count as i128 * coins * UNIT - user_fee_total
            );
            assert_eq!(native.balance(&f.collector), project_fees + user_fee_total);
            for p in &f.projects {
                assert_eq!(native.balance(p), 0);
            }
            for c in checks {
                assert!(FuulProjectClient::new(&e, &c.project_address).claimed_proofs(&c.proof));
            }
        }
    }
}

#[test]
fn source_mixed_issued_and_native_claims_use_distinct_payout_balances() {
    for compiled in [false, true] {
        for bps in [0, 200] {
            let e = test_env();
            let f = setup(&e, compiled);
            fees(&f, 0, bps, 100_000);
            let mut checks = Vec::new(&e);
            for (n, (asset, amount)) in
                [(&f.currency, 100 * UNIT), (&f.native, 10 * UNIT)].into_iter().enumerate()
            {
                fund(&e, &f, asset, &f.projects[0], amount + fee(&e, amount, bps));
                checks.push_back(check(&e, &f, 0, asset, amount, n as u8 + 40));
            }
            let initial = TokenClient::new(&e, &f.native).balance(&f.payer);
            f.manager.claim(&f.payer, &checks);
            assert_claim_events(&e, &f, &checks);
            for (asset, amount) in [(&f.currency, 100 * UNIT), (&f.native, 10 * UNIT)] {
                let token = TokenClient::new(&e, asset);
                assert_eq!(token.balance(&f.recipient), amount);
                assert_eq!(token.balance(&f.projects[0]), 0);
                assert_eq!(
                    token.balance(&f.collector),
                    fee(&e, amount, bps) + if asset == &f.native { 200_000 } else { 0 }
                );
            }
            assert_eq!(TokenClient::new(&e, &f.native).balance(&f.payer), initial - 200_000);
        }
    }
}

#[test]
fn source_distinct_exempt_and_nonexempt_callers_share_one_project() {
    for compiled in [false, true] {
        let e = test_env();
        let f = setup(&e, compiled);
        fees(&f, 0, 0, 100_000);
        let other = Address::generate(&e);
        fund(&e, &f, &f.native, &other, UNIT);
        fund(&e, &f, &f.native, &f.projects[0], 6 * UNIT);
        f.manager.add_no_claim_fee_address(&f.admin, &f.payer);
        for (n, caller) in [&f.payer, &other].into_iter().enumerate() {
            let native = TokenClient::new(&e, &f.native);
            let initial = native.balance(caller);
            let mut c = check(&e, &f, 0, &f.native, 3 * UNIT, n as u8 + 50);
            c.to = caller.clone();
            f.manager.claim(caller, &vec![&e, c]);
            assert_eq!(
                native.balance(caller),
                initial + 3 * UNIT - if n == 0 { 0 } else { 100_000 }
            );
            assert_eq!(native.balance(&f.collector), if n == 0 { 0 } else { 100_000 });
        }
    }
}

#[test]
fn source_kyc_provider_transition_controls_real_project_payout_and_retry() {
    for compiled in [false, true] {
        let e = test_env();
        let f = setup(&e, compiled);
        fees(&f, 0, 0, 0);
        let p = FuulProjectClient::new(&e, &f.projects[0]);
        p.set_kyc_required(&f.admin, &true);
        fund(&e, &f, &f.currency, &f.projects[0], 75 * UNIT);
        let c = check(&e, &f, 0, &f.currency, 75 * UNIT, 60);
        let checks = vec![&e, c.clone()];
        assert_eq!(f.manager.kyc_validator(), None);
        let before = e.to_ledger_snapshot().ledger_entries;
        assert_eq!(
            f.manager.try_claim(&f.payer, &checks),
            Err(Ok(Error::from_contract_error(6103)))
        );
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        let validator = e.register(MockKyc, (f.recipient.clone(),));
        f.manager.set_kyc_validator(&f.admin, &Some(validator.clone()));
        assert_eq!(f.manager.kyc_validator(), Some(validator));
        f.manager.claim(&f.payer, &checks);
        assert_claim_events(&e, &f, &checks);
        assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.recipient), 75 * UNIT);
        assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.collector), 0);
        assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.projects[0]), 0);
        assert!(p.claimed_proofs(&c.proof));
    }
}
