use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ProjectError {
    EmptyUri = 6100,
    Unauthorized = 6101,
    ProofAlreadyClaimed = 6102,
    KycRequiredForClaim = 6103,
    InvalidArgument = 6105,
    FeeCalculationFailed = 6107,
}
