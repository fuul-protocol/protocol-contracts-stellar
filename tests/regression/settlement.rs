use super::*;

#[test]
fn claim_settles_exact_fees_accounting_and_event_then_rejects_replay() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let c = f.check(1, 100_000);
    f.claim(&c);
    assert_eq!(
        e.events().all().filter_by_contract(&f.manager.address),
        std::vec![crate::Claimed {
            project_address: c.project_address.clone(),
            to: c.to.clone(),
            currency: c.currency.clone(),
            amount: c.amount,
            currency_type: c.currency_type,
            token_id: c.token_id.clone(),
            reason: c.reason,
            proof: c.proof.clone(),
        }
        .to_xdr(&e, &f.manager.address)]
    );
    assert_eq!(f.balances(), [899_000, 100_000, 1_000, 980_000, 20_000]);
    assert_eq!(f.manager.users_claims(&f.recipient, &f.currency), u(&e, 100_000));
    assert_eq!(
        f.manager.currency_limits(&f.currency).cumulative_claim_per_cooldown,
        u(&e, 100_000)
    );
    assert!(f.project.claimed_proofs(&c.proof));
    assert_eq!(f.manager.try_claim(&f.caller, &vec![&e, c]), Err(Ok(error(6102))));
    assert_eq!(f.balances(), [899_000, 100_000, 1_000, 980_000, 20_000]);
}

#[test]
fn invalid_claims_roll_back_all_accounting_and_transfers() {
    for case in 0..7 {
        let e = Env::default();
        let f = Fixture::new(&e);
        let mut c = f.check(2, 100);
        let code = match case {
            0 => {
                c.amount = -1;
                6300
            }
            1 => {
                c.amount = 1_000_001;
                6304
            }
            2 => {
                c.deadline = u(&e, 999_999);
                6305
            }
            3 => {
                c.signers = vec![&e];
                6306
            }
            4 => {
                c.signers = vec![&e, Address::generate(&e)];
                6307
            }
            5 => {
                c.signers.push_back(f.signer.clone());
                6301
            }
            _ => {
                f.project.set_kyc_required(&f.project_admin, &true);
                6103
            }
        };
        assert_eq!(
            f.manager.try_claim(&f.caller, &vec![&e, c.clone()]),
            Err(Ok(error(code))),
            "case {case}"
        );
        assert!(e.events().all().events().is_empty());
        f.assert_unsettled(&c);
    }
}

#[test]
fn failure_in_second_claim_rolls_back_first_claim_and_native_fee() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let first = f.check(3, 100);
    let mut second = f.check(4, 100);
    second.deadline = u(&e, 0);
    assert_eq!(
        f.manager.try_claim(&f.caller, &vec![&e, first.clone(), second.clone()]),
        Err(Ok(error(6305)))
    );
    assert!(e.events().all().events().is_empty());
    f.assert_unsettled(&first);
    f.assert_unsettled(&second);
}

#[test]
fn unpaid_native_fee_rolls_back_successful_project_transfer() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let c = f.check(5, 100);
    let empty_payer = Address::generate(&e);
    assert!(f.manager.try_claim(&empty_payer, &vec![&e, c.clone()]).is_err());
    assert!(e.events().all().events().is_empty());
    f.assert_unsettled(&c);
}

#[test]
fn insufficient_project_fee_balance_rolls_back_recipient_transfer() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let c = f.check(6, 1_000_000);
    assert!(f.manager.try_claim(&f.caller, &vec![&e, c.clone()]).is_err());
    assert!(e.events().all().events().is_empty());
    f.assert_unsettled(&c);
}

#[test]
fn cooldown_and_deadline_boundaries_are_inclusive() {
    let e = Env::default();
    let f = Fixture::new(&e);
    f.manager.set_currency_token_limit(&f.admin, &f.currency, &u(&e, 100));
    let mut c = f.check(7, 100);
    c.deadline = u(&e, 1_000_000);
    f.claim(&c);
    e.ledger().with_mut(|l| l.timestamp += 86_399);
    let mut next = f.check(8, 100);
    assert_eq!(f.manager.try_claim(&f.caller, &vec![&e, next.clone()]), Err(Ok(error(6304))));
    e.ledger().with_mut(|l| l.timestamp += 1);
    next.deadline = u(&e, 1_086_400);
    f.claim(&next);
    assert_eq!(f.manager.users_claims(&f.recipient, &f.currency), u(&e, 200));
    let limit = f.manager.currency_limits(&f.currency);
    assert_eq!(limit.cumulative_claim_per_cooldown, u(&e, 100));
    assert_eq!(limit.claim_cooldown_period_started, 1_086_400);
}

#[test]
fn fee_exemption_only_skips_the_callers_native_fee() {
    let e = Env::default();
    let f = Fixture::new(&e);
    f.manager.add_no_claim_fee_address(&f.admin, &f.caller);
    f.claim(&f.check(9, 100));
    assert_eq!(f.balances(), [999_899, 100, 1, 1_000_000, 0]);
    f.manager.remove_no_claim_fee_address(&f.admin, &f.caller);
    f.claim(&f.check(10, 100));
    assert_eq!(f.balances(), [999_798, 200, 2, 980_000, 20_000]);
}

#[contract]
struct Kyc;
#[contractimpl]
impl Kyc {
    pub fn is_user_kyc_registered(e: Env, user: Address) -> bool {
        e.storage().instance().get(&user).unwrap_or(false)
    }
    pub fn allow(e: Env, user: Address) {
        e.storage().instance().set(&user, &true);
    }
}

#[test]
fn kyc_requires_the_configured_validator_to_approve_the_recipient() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let c = f.check(11, 100);
    let kyc = KycClient::new(&e, &e.register(Kyc, ()));
    f.project.set_kyc_required(&f.project_admin, &true);
    f.manager.set_kyc_validator(&f.admin, &Some(kyc.address.clone()));
    assert_eq!(f.manager.try_claim(&f.caller, &vec![&e, c.clone()]), Err(Ok(error(6103))));
    f.assert_unsettled(&c);
    kyc.allow(&f.recipient);
    f.claim(&c);
    assert!(f.project.claimed_proofs(&c.proof));
}

#[test]
fn withdrawals_charge_from_the_amount_and_reject_invalid_inputs_atomically() {
    let e = Env::default();
    let f = Fixture::new(&e);
    f.factory.set_remove_fee(&f.factory_admin, &f.project.address, &250);
    for (caller, amount, code) in [(&f.caller, 100, 2000), (&f.project_admin, -1, 6105)] {
        assert_eq!(
            f.project.try_remove_funds(
                caller,
                &f.recipient,
                &f.currency,
                &TokenType::StellarAsset,
                &amount,
                &vec![&e],
                &vec![&e]
            ),
            Err(Ok(error(code)))
        );
    }
    f.assert_unsettled(&f.check(12, 100));
    f.project.remove_funds(
        &f.project_admin,
        &f.recipient,
        &f.currency,
        &TokenType::StellarAsset,
        &10_000,
        &vec![&e],
        &vec![&e],
    );
    assert_eq!(f.balances(), [990_000, 9_750, 250, 1_000_000, 0]);
}

#[test]
fn basis_point_rounding_and_full_i128_domain_are_safe() {
    use fuul_core::{calculate_basis_points as fee, FeeMathError};
    let e = Env::default();
    for (amount, bps, expected) in [
        (100_000, 0, 0),
        (100_000, 100, 1_000),
        (99, 100, 0),
        (100_000, 10_000, 100_000),
        (i128::MAX, 10_000, i128::MAX),
    ] {
        assert_eq!(fee(&e, amount, bps), Ok(expected));
    }
    assert_eq!(fee(&e, -1, 100), Err(FeeMathError::NegativeAmount));
    assert_eq!(fee(&e, 1, 10_001), Err(FeeMathError::InvalidBasisPoints));
}

#[contract]
struct Callback;
#[contractimpl]
impl Callback {
    pub fn enable(e: Env, enabled: bool) {
        e.storage().instance().set(&symbol_short!("enabled"), &enabled);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn claim(
        e: Env,
        manager: Address,
        to: Address,
        _currency: Address,
        _kind: TokenType,
        _amount: i128,
        _id: U256,
        _proof: BytesN<32>,
        _kyc: bool,
    ) -> fuul_core::ProjectClaimResult {
        manager.require_auth();
        if e.storage().instance().get(&symbol_short!("enabled")).unwrap_or(false) {
            FuulManagerClient::new(&e, &manager).claim(&to, &vec![&e]);
        }
        fuul_core::ProjectClaimResult { native_user_claim_fee: 0, fee_collector: to }
    }
}

#[test]
fn contract_callback_cannot_reenter_manager_and_rolls_back_earlier_claims() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let first = f.check(60, 100);
    let callback = CallbackClient::new(&e, &e.register(Callback, ()));
    let mut second = f.check(61, 100);
    second.project_address = callback.address.clone();
    callback.enable(&true);
    assert!(f.manager.try_claim(&f.caller, &vec![&e, first.clone(), second.clone()]).is_err());
    assert!(std::format!("{:?}", e.host().get_diagnostic_events().unwrap())
        .contains("Contract re-entry is not allowed"));
    assert!(e.events().all().events().is_empty());
    f.assert_unsettled(&first);
    // A zero-fee first claim avoids the source protocol's last-project collector semantics here.
    f.factory.set_native_user_claim_fee(&f.factory_admin, &f.project.address, &0);
    callback.enable(&false);
    f.manager.claim(&f.caller, &vec![&e, first, second]);
    assert_eq!(f.manager.users_claims(&f.recipient, &f.currency), u(&e, 200));
}

#[test]
fn proof_replay_is_scoped_to_each_project() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let mut c = f.check(62, 100);
    f.claim(&c);
    let next =
        f.factory.create_fuul_project(&f.project_admin, &String::from_str(&e, "next"), &false);
    StellarAssetClient::new(&e, &f.currency).mint(&next, &101);
    c.project_address = next.clone();
    f.claim(&c);
    assert!(FuulProjectClient::new(&e, &next).claimed_proofs(&c.proof));
    assert_eq!(f.manager.users_claims(&f.recipient, &f.currency), u(&e, 200));
}
