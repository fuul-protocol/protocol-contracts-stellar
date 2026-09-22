use crate::test::{constructor_helpers::*, roles_pause_helpers::test_env, *};

#[test]
fn every_invalid_bootstrap_guard_leaves_business_storage_and_events_empty() {
    for scenario in 0..5 {
        let env = test_env();
        let mut input = Bootstrap::new(&env);
        let code = match scenario {
            0 => {
                input.quorum = 0;
                6300
            }
            1 => {
                input.signers = Vec::new(&env);
                6300
            }
            2 => {
                input.quorum = 2;
                6300
            }
            3 => {
                input.signers.push_back(input.signers.get(0).unwrap());
                6301
            }
            _ => {
                input.native = input.accepted.clone();
                6302
            }
        };
        reject_native(&env, &input, code);
    }
}

#[test]
fn invalid_quorum_precedes_duplicate_signers_and_aliased_assets() {
    for quorum in [0, 3, 1_u128 << 96, u128::MAX] {
        let env = test_env();
        let mut input = Bootstrap::new(&env);
        input.signers.push_back(input.signers.get(0).unwrap());
        input.quorum = quorum;
        input.native = input.accepted.clone();
        reject_native(&env, &input, 6300);
    }
}

#[test]
fn duplicate_signer_precedes_aliased_assets() {
    let env = test_env();
    let mut input = Bootstrap::new(&env);
    input.signers.push_back(input.signers.get(0).unwrap());
    input.native = input.accepted.clone();
    reject_native(&env, &input, 6301);
}
