use crate::test::*;
use soroban_sdk::{
    testutils::{MockAuth, MockAuthInvoke},
    Val,
};

pub const PROJECT_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_project.wasm"
));

pub fn fixture(e: &Env, guest: bool, claim_fee: u32, remove_fee: u32) -> ClaimFixture<'_> {
    let f = configured_project_fixture(
        e,
        guest,
        false,
        ProjectFees { native_user_claim_fee: 0, project_claim_fee: claim_fee, remove_fee },
        1_000_000,
    );
    e.mock_auths(&[]);
    f
}

pub fn authorize(e: &Env, id: &Address, actor: &Address, name: &str, args: Vec<Val>) {
    e.mock_auths(&[MockAuth {
        address: actor,
        invoke: &MockAuthInvoke { contract: id, fn_name: name, args, sub_invokes: &[] },
    }]);
}
