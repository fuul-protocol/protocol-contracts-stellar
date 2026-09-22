use soroban_sdk::{contracterror, Env};
use stellar_contract_utils::math::i128_fixed_point::checked_mul_div;

pub const BASIS_POINTS_DENOMINATOR: u32 = 10_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FeeMathError {
    NegativeAmount = 6000,
    InvalidBasisPoints = 6001,
    Overflow = 6002,
}

/// Floors a nonnegative amount in basis points using a wide intermediate.
pub fn calculate_basis_points(
    e: &Env,
    amount: i128,
    basis_points: u32,
) -> Result<i128, FeeMathError> {
    if amount < 0 {
        return Err(FeeMathError::NegativeAmount);
    }
    if basis_points > BASIS_POINTS_DENOMINATOR {
        return Err(FeeMathError::InvalidBasisPoints);
    }

    checked_mul_div(e, &amount, &i128::from(basis_points), &i128::from(BASIS_POINTS_DENOMINATOR))
        .ok_or(FeeMathError::Overflow)
}
