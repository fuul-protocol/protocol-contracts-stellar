use super::{
    native_roots::Fixture,
    real_accounts::{invocation, signed_entry},
};
use crate::test::{constructor_helpers::assert_diagnostic, *};
use soroban_sdk::{xdr, Bytes, Executable};

const WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_e2e_auth_roots_fixture.wasm"
));
const WASM_SHA256: &str = "892c7619b3d03bc914baac5ba532e54affa54859e2d4c3291051dbbfa9b1eb9b";

fn fixture(overlap: bool) -> Fixture {
    let f = Fixture::with_wasm(overlap, Some(WASM));
    let digest = f.env.crypto().sha256(&Bytes::from_slice(&f.env, WASM)).to_array();
    let actual: std::string::String = digest.iter().map(|b| std::format!("{b:02x}")).collect();
    assert_eq!(actual, WASM_SHA256, "rebuild and identify the guest probe before testing");
    let executable = Some(Executable::Wasm(BytesN::from_array(&f.env, &digest)));
    assert_eq!(f.probe.executable(), executable);
    assert_eq!(f.payment.executable(), executable);
    assert_ne!(f.probe, f.payment);
    f
}

fn invocation_error() -> Error {
    Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)
}

#[test]
fn guest_recording_checks_exact_roots_and_handle_reuse_errors() {
    for overlap in [false, true] {
        for fresh in [false, true] {
            let f = fixture(overlap);
            let before = f.env.to_ledger_snapshot().ledger_entries;
            f.env.host().switch_to_recording_auth(true).unwrap();
            if !fresh {
                assert_eq!(f.call(fresh), Err(Ok(invocation_error())));
                assert_diagnostic(&f.env, xdr::ScError::Auth(ScErrorCode::ExistingValue));
                assert_eq!(f.env.to_ledger_snapshot().ledger_entries, before);
                assert!(f.env.events().all().events().is_empty());
                continue;
            }
            assert_eq!(f.call(fresh), Ok(()));
            let recorded = f.env.host().get_recorded_auth_payloads().unwrap();
            assert_eq!(recorded.len(), 3);
            assert_eq!(recorded[0].address, Some((&f.caller).into()));
            assert_eq!(recorded[0].invocation, f.caller_root(fresh));
            for i in 0..2 {
                assert_eq!(recorded[i + 1].address, Some((&f.signer).into()));
                assert_eq!(
                    recorded[i + 1].invocation,
                    invocation(&f.probe, (f.intents.get(i as u32).unwrap(),).into_val(&f.env))
                );
                assert!(recorded[i + 1].nonce.is_some());
            }
            assert_ne!(recorded[1].nonce, recorded[2].nonce);
            assert_eq!(f.env.events().all().events().len(), 2);
            f.assert_success();
        }
    }
}

#[test]
fn guest_enforcing_accepts_distinct_signed_roots_with_both_handle_variants() {
    for overlap in [false, true] {
        for fresh in [false, true] {
            let f = fixture(overlap);
            f.env.set_auths(&f.entries(fresh));
            assert_eq!(f.call(fresh), Ok(()));
            assert_eq!(f.env.events().all().events().len(), 2);
            f.assert_success();
        }
    }
}

#[test]
fn guest_negative_roots_revert_proofs_balances_events_and_nonces() {
    for case in 0..8 {
        let f = fixture(case == 2);
        if case == 2 {
            f.env.as_contract(&f.probe, || {
                f.env.storage().instance().set(&symbol_short!("exempt"), &f.caller)
            });
        }
        let valid = f.entries(true);
        let mut bad = valid.clone();
        match case {
            0 => {
                bad.pop();
            }
            1 | 2 => {
                bad.remove(0);
            }
            3 => {
                let mut altered = f.intents.get(1).unwrap();
                altered.set(symbol_short!("amount"), 21_i128.into_val(&f.env));
                bad[2] = signed_entry(
                    &f.env,
                    &f.signer,
                    &[&f.signer_key],
                    invocation(&f.probe, (altered,).into_val(&f.env)),
                    302,
                    200,
                    [5; 32],
                );
            }
            4 | 5 => {
                let mut root = f.caller_root(true);
                if case == 4 {
                    root.sub_invocations = Default::default();
                } else {
                    let mut child = root.sub_invocations[0].clone();
                    let xdr::SorobanAuthorizedFunction::ContractFn(c) = &mut child.function else {
                        unreachable!()
                    };
                    let args: Vec<Val> = (&f.caller, &f.collector, 9_i128).into_val(&f.env);
                    c.args = args.into();
                    root.sub_invocations = std::vec![child].try_into().unwrap();
                }
                bad[0] = signed_entry(&f.env, &f.caller, &[&f.caller_key], root, 300, 200, [5; 32]);
            }
            6 => {
                bad[2] = signed_entry(
                    &f.env,
                    &f.signer,
                    &[&f.signer_key],
                    invocation(&f.probe, (f.intents.get(1).unwrap(),).into_val(&f.env)),
                    301,
                    200,
                    [5; 32],
                );
            }
            _ => {
                let mut root = f.caller_root(true);
                let mut proof_root = bad[1].root_invocation.clone();
                proof_root.sub_invocations = root.sub_invocations.clone();
                root.sub_invocations = Default::default();
                bad[0] = signed_entry(&f.env, &f.caller, &[&f.caller_key], root, 300, 200, [5; 32]);
                bad[1] = signed_entry(
                    &f.env,
                    &f.signer,
                    &[&f.signer_key],
                    proof_root,
                    301,
                    200,
                    [5; 32],
                );
            }
        }
        let before = f.env.to_ledger_snapshot().ledger_entries;
        f.env.set_auths(&bad);
        let code = if case == 6 { ScErrorCode::ExistingValue } else { ScErrorCode::InvalidAction };
        assert_eq!(f.call(true), Err(Ok(invocation_error())), "case {case}");
        assert_diagnostic(&f.env, xdr::ScError::Auth(code));
        assert_eq!(f.env.to_ledger_snapshot().ledger_entries, before, "case {case}");
        assert!(f.env.events().all().events().is_empty());
        f.env.set_auths(&valid);
        assert_eq!(f.call(true), Ok(()), "same credentials after rollback, case {case}");
        if case != 2 {
            f.assert_success();
        }
    }
}

#[test]
fn guest_unconsumed_claim_entries_allow_caller_switch_and_reordering() {
    let mut f = fixture(false);
    let entries = f.entries(true);
    let before = f.env.to_ledger_snapshot().ledger_entries;
    f.env.set_auths(&entries[1..]);
    assert_eq!(f.call(true), Err(Ok(invocation_error())));
    assert_diagnostic(&f.env, xdr::ScError::Auth(ScErrorCode::InvalidAction));
    assert_eq!(f.env.to_ledger_snapshot().ledger_entries, before);
    assert!(f.env.events().all().events().is_empty());
    f.caller = f.signer.clone();
    f.intents = vec![&f.env, f.intents.get(1).unwrap(), f.intents.get(0).unwrap()];
    let mut changed = f.entries(true);
    changed[1] = entries[1].clone();
    changed[2] = entries[2].clone();
    f.env.set_auths(&changed);
    assert_eq!(f.call(true), Ok(()));
    f.assert_success();
}
