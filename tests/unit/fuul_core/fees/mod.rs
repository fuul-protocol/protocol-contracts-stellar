use crate::fees::*;

#[test]
fn calculates_zero_fee() {
    assert_eq!(calculate_basis_points(&soroban_sdk::Env::default(), 100_000, 0), Ok(0));
}

#[test]
fn calculates_one_percent_fee() {
    assert_eq!(calculate_basis_points(&soroban_sdk::Env::default(), 100_000, 100), Ok(1_000));
}

#[test]
fn rounds_down_like_the_source_contract() {
    assert_eq!(calculate_basis_points(&soroban_sdk::Env::default(), 99, 100), Ok(0));
}

#[test]
fn accepts_full_amount_fee() {
    assert_eq!(calculate_basis_points(&soroban_sdk::Env::default(), 100_000, 10_000), Ok(100_000));
}

#[test]
fn rejects_negative_amount() {
    assert_eq!(
        calculate_basis_points(&soroban_sdk::Env::default(), -1, 100),
        Err(FeeMathError::NegativeAmount)
    );
}

#[test]
fn rejects_basis_points_above_one_hundred_percent() {
    assert_eq!(
        calculate_basis_points(&soroban_sdk::Env::default(), 100_000, 10_001),
        Err(FeeMathError::InvalidBasisPoints)
    );
}

#[test]
fn accepts_full_range_when_the_quotient_fits() {
    assert_eq!(
        calculate_basis_points(&soroban_sdk::Env::default(), i128::MAX, 10_000),
        Ok(i128::MAX)
    );
}
