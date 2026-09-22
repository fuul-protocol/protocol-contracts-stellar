use crate::test::{
    constructor_helpers::assert_diagnostic,
    roles_pause_helpers as auth,
    security_helpers::{claim_payload, state},
    *,
};
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    xdr::{self, Limits, WriteXdr},
    Bytes, Map, TryFromVal,
};
use std::rc::Rc;
mod gaps;

pub(super) fn account(
    env: &Env,
    key: &SigningKey,
    secondary: Option<&SigningKey>,
    threshold: u8,
) -> Address {
    let id = xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(
        key.verifying_key().to_bytes(),
    )));
    let signers = secondary
        .into_iter()
        .map(|key| xdr::Signer {
            key: xdr::SignerKey::Ed25519(xdr::Uint256(key.verifying_key().to_bytes())),
            weight: 1,
        })
        .collect::<std::vec::Vec<_>>();
    let ledger_key =
        Rc::new(xdr::LedgerKey::Account(xdr::LedgerKeyAccount { account_id: id.clone() }));
    let entry = Rc::new(xdr::LedgerEntry {
        last_modified_ledger_seq: 0,
        ext: xdr::LedgerEntryExt::V0,
        data: xdr::LedgerEntryData::Account(xdr::AccountEntry {
            account_id: id.clone(),
            balance: 0,
            seq_num: xdr::SequenceNumber(0),
            num_sub_entries: signers.len() as u32,
            inflation_dest: None,
            flags: 0,
            home_domain: Default::default(),
            thresholds: xdr::Thresholds([1, 1, threshold, 1]),
            signers: signers.try_into().unwrap(),
            ext: xdr::AccountEntryExt::V0,
        }),
    });
    env.host().add_ledger_entry(&ledger_key, &entry, None).unwrap();
    Address::try_from_val(env, &xdr::ScAddress::Account(id)).unwrap()
}

pub(super) fn invocation(manager: &Address, args: Vec<Val>) -> xdr::SorobanAuthorizedInvocation {
    xdr::SorobanAuthorizedInvocation {
        function: xdr::SorobanAuthorizedFunction::ContractFn(xdr::InvokeContractArgs {
            contract_address: manager.into(),
            function_name: "claim".try_into().unwrap(),
            args: args.into(),
        }),
        sub_invocations: Default::default(),
    }
}

pub(super) fn signed_entry(
    env: &Env,
    address: &Address,
    keys: &[&SigningKey],
    root: xdr::SorobanAuthorizedInvocation,
    nonce: i64,
    expiration: u32,
    network: [u8; 32],
) -> SorobanAuthorizationEntry {
    let preimage =
        xdr::HashIdPreimage::SorobanAuthorization(xdr::HashIdPreimageSorobanAuthorization {
            network_id: xdr::Hash(network),
            nonce,
            signature_expiration_ledger: expiration,
            invocation: root.clone(),
        });
    let digest = env
        .crypto()
        .sha256(&Bytes::from_slice(env, &preimage.to_xdr(Limits::none()).unwrap()))
        .to_array();
    let mut sorted = keys.to_vec();
    sorted.sort_by_key(|key| key.verifying_key().to_bytes());
    let mut signatures = Vec::<Val>::new(env);
    for key in sorted {
        signatures.push_back(
            Map::<Symbol, Val>::from_array(
                env,
                [
                    (
                        Symbol::new(env, "public_key"),
                        BytesN::from_array(env, &key.verifying_key().to_bytes()).into_val(env),
                    ),
                    (
                        Symbol::new(env, "signature"),
                        BytesN::from_array(env, &key.sign(&digest).to_bytes()).into_val(env),
                    ),
                ],
            )
            .into_val(env),
        );
    }
    SorobanAuthorizationEntry {
        root_invocation: root,
        credentials: xdr::SorobanCredentials::Address(xdr::SorobanAddressCredentials {
            address: address.into(),
            nonce,
            signature_expiration_ledger: expiration,
            signature: xdr::ScVal::try_from_val(env, &signatures.to_val()).unwrap(),
        }),
    }
}

pub(super) fn setup<'a>(
    env: &'a Env,
    signer: &Address,
) -> (FuulManagerClient<'a>, Vec<ClaimCheck>) {
    env.ledger().with_mut(|l| {
        l.timestamp = 1_000_000;
        l.sequence_number = 100;
        l.network_id = [5; 32];
    });
    let currency = Address::generate(env);
    let (manager, _, _, _) = register_manager(
        env,
        1,
        vec![env, signer.clone()],
        &currency,
        &Address::generate(env),
        None,
    );
    let project = register_mock_project(env, signer, 0);
    let checks = vec![
        env,
        ClaimCheck {
            project_address: project.address,
            to: signer.clone(),
            currency,
            currency_type: TokenType::StellarAsset,
            amount: 10,
            reason: ClaimReason::AffiliatePayout,
            token_id: u(env, 0),
            deadline: u(env, 1_000_300),
            proof: BytesN::from_array(env, &[94; 32]),
            signers: vec![env, signer.clone()],
        },
    ];
    (manager, checks)
}

pub(super) fn set_claim_auths(
    env: &Env,
    manager: &Address,
    signer: &Address,
    keys: &[&SigningKey],
    checks: &Vec<ClaimCheck>,
    proof_entries: &[SorobanAuthorizationEntry],
) {
    static NEXT_CALLER_NONCE: std::sync::atomic::AtomicI64 =
        std::sync::atomic::AtomicI64::new(100_000);
    let nonce = NEXT_CALLER_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut caller = signed_entry(
        env,
        signer,
        keys,
        invocation(manager, (signer, checks).into_val(env)),
        nonce,
        200,
        [5; 32],
    );
    if matches!(xdr::ScAddress::from(signer), xdr::ScAddress::Contract(_)) {
        let xdr::SorobanCredentials::Address(c) = &mut caller.credentials else { unreachable!() };
        let signatures = Vec::<Map<Symbol, Val>>::try_from_val(env, &c.signature).unwrap();
        let signature = signatures.get(0).unwrap().get(Symbol::new(env, "signature")).unwrap();
        c.signature = xdr::ScVal::try_from_val(env, &signature).unwrap();
    }
    let mut entries = std::vec![caller];
    entries.extend_from_slice(proof_entries);
    env.set_auths(&entries);
}

#[test]
fn real_g_account_weights_require_medium_threshold_and_accept_two_signatures() {
    let env = auth::test_env();
    // Fixed TEST-ONLY keys: no wallet or network account is involved.
    let master = SigningKey::from_bytes(&[11; 32]);
    let secondary = SigningKey::from_bytes(&[12; 32]);
    let signer = account(&env, &master, Some(&secondary), 2);
    let (manager, checks) = setup(&env, &signer);
    let root = invocation(&manager.address, claim_payload(&env, &checks.get(0).unwrap()));
    let before = state(&env, &manager.address);
    set_claim_auths(
        &env,
        &manager.address,
        &signer,
        &[&master, &secondary],
        &checks,
        &[signed_entry(&env, &signer, &[&master], root.clone(), 101, 200, [5; 32])],
    );
    assert_eq!(manager.try_claim(&signer, &checks), Err(Ok(auth::native_auth_error())));
    assert_diagnostic(&env, xdr::ScError::Auth(ScErrorCode::InvalidAction));
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &manager.address), before);
    set_claim_auths(
        &env,
        &manager.address,
        &signer,
        &[&master, &secondary],
        &checks,
        &[signed_entry(&env, &signer, &[&master, &secondary], root, 101, 200, [5; 32])],
    );
    manager.claim(&signer, &checks);
    assert_eq!(manager.users_claims(&signer, &checks.get(0).unwrap().currency), u(&env, 10));
}

#[test]
fn real_g_account_rejects_signature_network_expiry_and_invocation_tampering() {
    for mutation in 0..6 {
        let env = auth::test_env();
        let key = SigningKey::from_bytes(&[21; 32]);
        let signer = account(&env, &key, None, 1);
        let (manager, checks) = setup(&env, &signer);
        let root = invocation(&manager.address, claim_payload(&env, &checks.get(0).unwrap()));
        let valid = signed_entry(&env, &signer, &[&key], root.clone(), 102, 200, [5; 32]);
        let mut bad = valid.clone();
        match mutation {
            0 => {
                if let xdr::SorobanCredentials::Address(c) = &mut bad.credentials {
                    let mut signatures =
                        Vec::<Map<Symbol, Val>>::try_from_val(&env, &c.signature).unwrap();
                    let mut first = signatures.get(0).unwrap();
                    let field = Symbol::new(&env, "signature");
                    let mut bytes =
                        BytesN::<64>::try_from_val(&env, &first.get(field.clone()).unwrap())
                            .unwrap()
                            .to_array();
                    bytes[0] ^= 1;
                    first.set(field, BytesN::from_array(&env, &bytes).into_val(&env));
                    signatures.set(0, first);
                    c.signature = xdr::ScVal::try_from_val(&env, &signatures.to_val()).unwrap();
                }
            }
            1 => bad = signed_entry(&env, &signer, &[&key], root.clone(), 102, 200, [6; 32]),
            2 => bad = signed_entry(&env, &signer, &[&key], root.clone(), 102, 99, [5; 32]),
            3 => {
                if let xdr::SorobanAuthorizedFunction::ContractFn(f) =
                    &mut bad.root_invocation.function
                {
                    f.contract_address = (&Address::generate(&env)).into();
                }
            }
            4 => {
                if let xdr::SorobanAuthorizedFunction::ContractFn(f) =
                    &mut bad.root_invocation.function
                {
                    f.function_name = "other_claim".try_into().unwrap();
                }
            }
            _ => {
                if let xdr::SorobanCredentials::Address(c) = &mut bad.credentials {
                    c.nonce += 1;
                }
            }
        }
        let before = state(&env, &manager.address);
        set_claim_auths(&env, &manager.address, &signer, &[&key], &checks, &[bad]);
        assert_eq!(
            manager.try_claim(&signer, &checks),
            Err(Ok(auth::native_auth_error())),
            "mutation {mutation}"
        );
        assert_diagnostic(
            &env,
            xdr::ScError::Auth(if mutation == 2 {
                ScErrorCode::InvalidInput
            } else {
                ScErrorCode::InvalidAction
            }),
        );
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &manager.address), before);
        set_claim_auths(&env, &manager.address, &signer, &[&key], &checks, &[valid]);
        manager.claim(&signer, &checks);
        assert_eq!(manager.users_claims(&signer, &checks.get(0).unwrap().currency), u(&env, 10));
    }
}

#[test]
fn real_g_account_nonce_cannot_be_reused_and_expiration_ledger_is_inclusive() {
    let env = auth::test_env();
    let key = SigningKey::from_bytes(&[31; 32]);
    let signer = account(&env, &key, None, 1);
    let (manager, checks) = setup(&env, &signer);
    let root = invocation(&manager.address, claim_payload(&env, &checks.get(0).unwrap()));
    let entry = signed_entry(&env, &signer, &[&key], root, 103, 100, [5; 32]);
    set_claim_auths(
        &env,
        &manager.address,
        &signer,
        &[&key],
        &checks,
        core::slice::from_ref(&entry),
    );
    manager.claim(&signer, &checks);
    let before = state(&env, &manager.address);
    set_claim_auths(&env, &manager.address, &signer, &[&key], &checks, &[entry]);
    assert_eq!(manager.try_claim(&signer, &checks), Err(Ok(auth::native_auth_error())));
    assert_diagnostic(&env, xdr::ScError::Auth(ScErrorCode::ExistingValue));
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &manager.address), before);
    assert_eq!(manager.users_claims(&signer, &checks.get(0).unwrap().currency), u(&env, 10));
}

#[contract]
struct SigningAccount;
#[contractimpl]
impl SigningAccount {
    pub fn __constructor(e: Env, key: BytesN<32>) {
        e.storage().instance().set(&symbol_short!("key"), &key);
    }
}
#[contractimpl]
impl CustomAccountInterface for SigningAccount {
    type Signature = BytesN<64>;
    type Error = Error;
    fn __check_auth(
        e: Env,
        payload: Hash<32>,
        signature: Self::Signature,
        contexts: Vec<Context>,
    ) -> Result<(), Error> {
        assert_eq!(contexts.len(), 1);
        let Context::Contract(context) = contexts.get(0).unwrap() else {
            panic!("expected claim context")
        };
        assert_eq!(context.fn_name, Symbol::new(&e, "claim"));
        let key: BytesN<32> = e.storage().instance().get(&symbol_short!("key")).unwrap();
        e.crypto().ed25519_verify(&key, &payload.to_bytes().into(), &signature);
        Ok(())
    }
}

#[contract]
struct ClaimInvoker;
#[contractimpl]
impl ClaimInvoker {
    pub fn execute(e: Env, manager: Address, checks: Vec<ClaimCheck>) {
        FuulManagerClient::new(&e, &manager).claim(&e.current_contract_address(), &checks);
    }
}

#[test]
fn valid_contract_account_verifies_signature_without_auth_mocks() {
    let env = auth::test_env();
    let key = SigningKey::from_bytes(&[41; 32]);
    let signer =
        env.register(SigningAccount, (BytesN::from_array(&env, &key.verifying_key().to_bytes()),));
    let (manager, checks) = setup(&env, &signer);
    let root = invocation(&manager.address, claim_payload(&env, &checks.get(0).unwrap()));
    let preimage =
        xdr::HashIdPreimage::SorobanAuthorization(xdr::HashIdPreimageSorobanAuthorization {
            network_id: xdr::Hash([5; 32]),
            nonce: 104,
            signature_expiration_ledger: 200,
            invocation: root.clone(),
        });
    let digest = env
        .crypto()
        .sha256(&Bytes::from_slice(&env, &preimage.to_xdr(Limits::none()).unwrap()))
        .to_array();
    let signature = BytesN::from_array(&env, &key.sign(&digest).to_bytes());
    set_claim_auths(
        &env,
        &manager.address,
        &signer,
        &[&key],
        &checks,
        &[SorobanAuthorizationEntry {
            root_invocation: root,
            credentials: xdr::SorobanCredentials::Address(xdr::SorobanAddressCredentials {
                address: (&signer).into(),
                nonce: 104,
                signature_expiration_ledger: 200,
                signature: xdr::ScVal::try_from_val(&env, &signature.to_val()).unwrap(),
            }),
        }],
    );
    manager.claim(&signer, &checks);
    assert_eq!(manager.users_claims(&signer, &checks.get(0).unwrap().currency), u(&env, 10));
}

#[test]
fn direct_contract_invoker_can_be_caller_and_signer_without_credentials() {
    let env = auth::test_env();
    let invoker = env.register(ClaimInvoker, ());
    let (manager, checks) = setup(&env, &invoker);
    env.set_auths(&[]);
    ClaimInvokerClient::new(&env, &invoker).execute(&manager.address, &checks);
    assert_eq!(manager.users_claims(&invoker, &checks.get(0).unwrap().currency), u(&env, 10));
}
