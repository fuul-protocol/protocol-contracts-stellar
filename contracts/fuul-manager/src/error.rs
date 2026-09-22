use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ManagerError {
    InvalidArgument = 6300,
    DuplicateSigner = 6301,
    LimitAlreadySet = 6302,
    LimitBelowCumulative = 6303,
    OverTheLimit = 6304,
    DeadlineExpired = 6305,
    NotEnoughSigners = 6306,
    InvalidSigner = 6307,
    AmountOverflow = 6308,
    IncorrectFee = 6309,
}
