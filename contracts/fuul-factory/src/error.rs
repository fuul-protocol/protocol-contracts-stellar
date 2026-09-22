use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FactoryError {
    EmptyUri = 6200,
    InvalidArgument = 6201,
    TrackerOverflow = 6202,
}
