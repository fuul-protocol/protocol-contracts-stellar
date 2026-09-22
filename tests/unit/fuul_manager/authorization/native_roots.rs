use super::real_accounts::{account, invocation, signed_entry};
use crate::test::{constructor_helpers::assert_diagnostic, roles_pause_helpers as auth, *};
use ed25519_dalek::SigningKey;
use soroban_sdk::{contractevent, xdr, Map};

// Native capability probes, not Manager implementations or token qualification.
#[contract]
struct RootProbe;

#[contractevent]
struct ProbeCompleted {
    count: u32,
}

#[contractimpl]
impl RootProbe {
    pub fn claim(
        e: Env,
        caller: Address,
        signer: Address,
        intents: Vec<Map<Symbol, Val>>,
        payment: Address,
        collector: Address,
        fresh: bool,
    ) {
        e.storage().instance().set(&symbol_short!("started"), &true);
        caller.require_auth();
        for intent in intents.iter() {
            let identity =
                if fresh { Address::from_string(&signer.to_string()) } else { signer.clone() };
            identity.require_auth_for_args((intent.clone(),).into_val(&e));
            let proof = intent.get(symbol_short!("proof")).unwrap();
            assert!(!e.storage().persistent().has(&proof));
            e.storage().persistent().set(&proof, &true);
            ProbeCompleted { count: 1 }.publish(&e);
        }
        let exempt: Option<Address> = e.storage().instance().get(&symbol_short!("exempt"));
        if exempt.as_ref() != Some(&caller) {
            PaymentProbeClient::new(&e, &payment).transfer(&caller, &collector, &10);
        }
    }
}

#[contract]
struct PaymentProbe;

#[contractimpl]
impl PaymentProbe {
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let balance: i128 = e.storage().persistent().get(&from).unwrap_or(100);
        assert!(amount >= 0 && balance >= amount);
        e.storage().persistent().set(&from, &(balance - amount));
        let received: i128 = e.storage().persistent().get(&to).unwrap_or(0);
        e.storage().persistent().set(&to, &(received + amount));
    }
}

pub(super) struct Fixture {
    pub(super) env: Env,
    pub(super) probe: Address,
    pub(super) payment: Address,
    pub(super) collector: Address,
    pub(super) signer: Address,
    pub(super) caller: Address,
    pub(super) signer_key: SigningKey,
    pub(super) caller_key: SigningKey,
    pub(super) intents: Vec<Map<Symbol, Val>>,
}

impl Fixture {
    fn new(overlap: bool) -> Self {
        Self::with_wasm(overlap, None)
    }

    pub(super) fn with_wasm(overlap: bool, wasm: Option<&[u8]>) -> Self {
        let env = auth::test_env();
        env.ledger().with_mut(|l| {
            l.sequence_number = 100;
            l.network_id = [5; 32];
        });
        let signer_key = SigningKey::from_bytes(&[71; 32]);
        let caller_key = SigningKey::from_bytes(&[72; 32]);
        let signer = account(&env, &signer_key, None, 1);
        let caller = if overlap { signer.clone() } else { account(&env, &caller_key, None, 1) };
        let (probe, payment) = if let Some(wasm) = wasm {
            (env.register(wasm, ()), env.register(wasm, ()))
        } else {
            (env.register(RootProbe, ()), env.register(PaymentProbe, ()))
        };
        let collector = Address::generate(&env);
        let mut intents = Vec::new(&env);
        for n in [1_u8, 2] {
            intents.push_back(Map::from_array(
                &env,
                [
                    (Symbol::new(&env, "project"), probe.clone().into_val(&env)),
                    (Symbol::new(&env, "to"), collector.clone().into_val(&env)),
                    (Symbol::new(&env, "currency"), payment.clone().into_val(&env)),
                    (Symbol::new(&env, "amount"), 20_i128.into_val(&env)),
                    (
                        Symbol::new(&env, "reason"),
                        (Symbol::new(&env, "AffiliatePayout"),).into_val(&env),
                    ),
                    (Symbol::new(&env, "token_id"), 0_i128.into_val(&env)),
                    (Symbol::new(&env, "deadline"), 1000_u64.into_val(&env)),
                    (Symbol::new(&env, "proof"), BytesN::from_array(&env, &[n; 32]).into_val(&env)),
                ],
            ));
        }
        Self { env, probe, payment, collector, signer, caller, signer_key, caller_key, intents }
    }

    pub(super) fn caller_root(&self, fresh: bool) -> xdr::SorobanAuthorizedInvocation {
        let mut root = invocation(
            &self.probe,
            (&self.caller, &self.signer, &self.intents, &self.payment, &self.collector, fresh)
                .into_val(&self.env),
        );
        let mut payment =
            invocation(&self.payment, (&self.caller, &self.collector, 10_i128).into_val(&self.env));
        let xdr::SorobanAuthorizedFunction::ContractFn(f) = &mut payment.function else {
            unreachable!()
        };
        f.function_name = "transfer".try_into().unwrap();
        root.sub_invocations = std::vec![payment].try_into().unwrap();
        root
    }

    pub(super) fn entries(&self, fresh: bool) -> std::vec::Vec<SorobanAuthorizationEntry> {
        let key = if self.caller == self.signer { &self.signer_key } else { &self.caller_key };
        let mut entries = std::vec![signed_entry(
            &self.env,
            &self.caller,
            &[key],
            self.caller_root(fresh),
            300,
            200,
            [5; 32],
        )];
        for (i, intent) in self.intents.iter().enumerate() {
            entries.push(signed_entry(
                &self.env,
                &self.signer,
                &[&self.signer_key],
                invocation(&self.probe, (intent,).into_val(&self.env)),
                301 + i as i64,
                200,
                [5; 32],
            ));
        }
        entries
    }

    pub(super) fn call(&self, fresh: bool) -> Result<(), Result<Error, soroban_sdk::InvokeError>> {
        RootProbeClient::new(&self.env, &self.probe)
            .try_claim(
                &self.caller,
                &self.signer,
                &self.intents,
                &self.payment,
                &self.collector,
                &fresh,
            )
            .map(|result| result.unwrap())
    }

    pub(super) fn assert_success(&self) {
        self.env.as_contract(&self.probe, || {
            assert_eq!(
                self.env.storage().instance().get::<_, bool>(&symbol_short!("started")),
                Some(true)
            );
            for intent in self.intents.iter() {
                assert_eq!(
                    self.env
                        .storage()
                        .persistent()
                        .get::<_, bool>(&intent.get(symbol_short!("proof")).unwrap()),
                    Some(true)
                );
            }
        });
        self.env.as_contract(&self.payment, || {
            assert_eq!(self.env.storage().persistent().get::<_, i128>(&self.caller), Some(90));
            assert_eq!(self.env.storage().persistent().get::<_, i128>(&self.collector), Some(10));
        });
    }
}

#[test]
fn recording_distinguishes_reused_handles_from_fresh_equivalent_addresses() {
    for overlap in [false, true] {
        for fresh in [false, true] {
            let f = Fixture::new(overlap);
            let before = f.env.to_ledger_snapshot().ledger_entries;
            f.env.host().switch_to_recording_auth(true).unwrap();
            let result = f.call(fresh);
            if !fresh {
                assert_eq!(result, Err(Ok(auth::native_auth_error())));
                assert_diagnostic(&f.env, xdr::ScError::Auth(ScErrorCode::ExistingValue));
                assert_eq!(f.env.to_ledger_snapshot().ledger_entries, before);
                assert!(f.env.events().all().events().is_empty());
                continue;
            }
            assert_eq!(result, Ok(()));
            let recorded = f.env.host().get_recorded_auth_payloads().unwrap();
            assert_eq!(recorded.len(), 3);
            assert_eq!(recorded[0].invocation, f.caller_root(fresh));
            assert_eq!(recorded[0].address, Some((&f.caller).into()));
            for i in 0..2 {
                assert_eq!(recorded[i + 1].address, Some((&f.signer).into()));
                assert_eq!(
                    recorded[i + 1].invocation,
                    invocation(&f.probe, (f.intents.get(i as u32).unwrap(),).into_val(&f.env))
                );
                assert!(recorded[i + 1].nonce.is_some());
            }
            f.assert_success();
        }
    }
}

#[test]
fn enforcing_accepts_independent_signed_roots_with_both_handle_variants() {
    for overlap in [false, true] {
        for fresh in [false, true] {
            let f = Fixture::new(overlap);
            f.env.set_auths(&f.entries(fresh));
            assert_eq!(f.call(fresh), Ok(()));
            assert_eq!(f.env.events().all().events().len(), 2);
            f.assert_success();
        }
    }
}

#[test]
fn missing_or_misbound_roots_revert_all_probe_state_events_and_nonces() {
    for case in 0..8 {
        let f = Fixture::new(case == 2);
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
                let mut caller_root = f.caller_root(true);
                let mut proof_root = bad[1].root_invocation.clone();
                proof_root.sub_invocations = caller_root.sub_invocations.clone();
                caller_root.sub_invocations = Default::default();
                bad[0] = signed_entry(
                    &f.env,
                    &f.caller,
                    &[&f.caller_key],
                    caller_root,
                    300,
                    200,
                    [5; 32],
                );
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
        assert_eq!(f.call(true), Err(Ok(auth::native_auth_error())), "case {case}");
        assert_diagnostic(
            &f.env,
            xdr::ScError::Auth(if case == 6 {
                ScErrorCode::ExistingValue
            } else {
                ScErrorCode::InvalidAction
            }),
        );
        assert_eq!(f.env.to_ledger_snapshot().ledger_entries, before, "case {case}");
        assert!(f.env.events().all().events().is_empty());
        f.env.set_auths(&valid);
        assert_eq!(f.call(true), Ok(()), "same credentials after rollback, case {case}");
    }
}

#[test]
fn unconsumed_signer_credentials_survive_a_change_of_caller_and_batch_order() {
    let mut f = Fixture::new(false);
    let entries = f.entries(true);
    let before = f.env.to_ledger_snapshot().ledger_entries;
    f.env.set_auths(&entries[1..]);
    assert_eq!(f.call(true), Err(Ok(auth::native_auth_error())));
    assert_eq!(f.env.to_ledger_snapshot().ledger_entries, before);
    f.caller = f.signer.clone();
    f.intents = vec![&f.env, f.intents.get(1).unwrap(), f.intents.get(0).unwrap()];
    let mut changed = f.entries(true);
    changed[1] = entries[1].clone();
    changed[2] = entries[2].clone();
    f.env.set_auths(&changed);
    assert_eq!(f.call(true), Ok(()));
    f.assert_success();
}
