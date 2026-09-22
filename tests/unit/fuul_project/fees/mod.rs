use crate::test::*;

#[test]
fn claim_accepts_zero_rounded_project_fees() {
    for (amount, bps) in [(1, 1), (99, 100), (0, 0), (0, 100)] {
        let e = Env::default();
        let f = claim_fixture(&e, false, bps, 0);
        let p = proof(&e, 70);
        f.client.claim(
            &f.manager,
            &f.recipient,
            &f.currency,
            &TokenType::StellarAsset,
            &amount,
            &u(&e, 0),
            &p,
            &false,
        );
        let token = TokenClient::new(&e, &f.currency);
        assert_eq!(token.balance(&f.recipient), amount);
        assert_eq!(token.balance(&f.collector), 0);
        assert!(f.client.claimed_proofs(&p));
    }
}

#[test]
fn zero_claim_with_positive_bps_consumes_the_authorized_proof() {
    let e = Env::default();
    let f = claim_fixture(&e, false, 100, 0);
    let p = proof(&e, 71);
    f.client.claim(
        &f.manager,
        &f.recipient,
        &f.currency,
        &TokenType::StellarAsset,
        &0,
        &u(&e, 0),
        &p,
        &false,
    );
    assert!(f.client.claimed_proofs(&p));
}

#[test]
fn removal_accepts_zero_and_zero_rounded_fees() {
    for (amount, bps) in [(0, 0), (0, 100), (1, 1), (99, 100)] {
        let e = Env::default();
        let f = project_fixture(&e, false, 0, 0, bps);
        f.client.remove_funds(
            &f.admin,
            &f.recipient,
            &f.currency,
            &TokenType::StellarAsset,
            &amount,
            &Vec::new(&e),
            &Vec::new(&e),
        );
        let token = TokenClient::new(&e, &f.currency);
        assert_eq!(token.balance(&f.recipient), amount);
        assert_eq!(token.balance(&f.collector), 0);
        assert_eq!(token.balance(&f.client.address), 1_000_000 - amount);
    }
}

#[test]
fn wide_intermediate_preserves_a_representable_fee() {
    let amount = i128::MAX / 10_000 + 1;
    assert_eq!(fuul_core::calculate_basis_points(&Env::default(), amount, 10_000), Ok(amount));
}

#[test]
fn wide_claim_and_removal_preserve_exact_balances() {
    for claim in [true, false] {
        let e = Env::default();
        let f = project_fixture(&e, false, 10_000, 0, 10_000);
        let amount = i128::MAX / 10_000 + 1;
        let funded = if claim { 2 * amount } else { amount };
        StellarAssetClient::new(&e, &f.currency).mint(&f.client.address, &(funded - 1_000_000));
        if claim {
            f.client.claim(
                &f.manager,
                &f.recipient,
                &f.currency,
                &TokenType::StellarAsset,
                &amount,
                &u(&e, 0),
                &proof(&e, 72),
                &false,
            );
        } else {
            f.client.remove_funds(
                &f.admin,
                &f.recipient,
                &f.currency,
                &TokenType::StellarAsset,
                &amount,
                &Vec::new(&e),
                &Vec::new(&e),
            );
        }
        let token = TokenClient::new(&e, &f.currency);
        assert_eq!(token.balance(&f.client.address), 0);
        assert_eq!(token.balance(&f.collector), amount);
        assert_eq!(token.balance(&f.recipient), if claim { amount } else { 0 });
    }
}
