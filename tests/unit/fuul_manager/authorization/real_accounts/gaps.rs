use super::*;

#[test]
fn duplicate_or_unknown_ed25519_keys_cannot_supply_account_threshold_weight() {
    for duplicate in [true, false] {
        let env = auth::test_env();
        let master = SigningKey::from_bytes(&[51; 32]);
        let secondary = SigningKey::from_bytes(&[52; 32]);
        let unknown = SigningKey::from_bytes(&[53; 32]);
        let signer = account(&env, &master, Some(&secondary), 2);
        let (manager, checks) = setup(&env, &signer);
        let root = invocation(&manager.address, claim_payload(&env, &checks.get(0).unwrap()));
        let bad_keys = [&master, if duplicate { &master } else { &unknown }];
        let before = state(&env, &manager.address);
        set_claim_auths(
            &env,
            &manager.address,
            &signer,
            &[&master, &secondary],
            &checks,
            &[signed_entry(&env, &signer, &bad_keys, root.clone(), 201, 200, [5; 32])],
        );
        assert_eq!(manager.try_claim(&signer, &checks), Err(Ok(auth::native_auth_error())));
        assert_diagnostic(&env, xdr::ScError::Auth(ScErrorCode::InvalidAction));
        let diagnostics = env.host().get_diagnostic_events().unwrap();
        assert!(std::format!("{diagnostics:?}").contains(if duplicate {
            "public keys are not ordered"
        } else {
            "signer does not belong to account"
        }));
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &manager.address), before);
        set_claim_auths(
            &env,
            &manager.address,
            &signer,
            &[&master, &secondary],
            &checks,
            &[signed_entry(&env, &signer, &[&master, &secondary], root, 201, 200, [5; 32])],
        );
        manager.claim(&signer, &checks);
        assert_eq!(manager.users_claims(&signer, &checks.get(0).unwrap().currency), u(&env, 10));
    }
}

#[test]
fn corrupt_contract_account_signature_is_rejected_before_valid_signature_control() {
    let env = auth::test_env();
    let key = SigningKey::from_bytes(&[54; 32]);
    let signer =
        env.register(SigningAccount, (BytesN::from_array(&env, &key.verifying_key().to_bytes()),));
    let (manager, checks) = setup(&env, &signer);
    let root = invocation(&manager.address, claim_payload(&env, &checks.get(0).unwrap()));
    let mut valid = signed_entry(&env, &signer, &[&key], root, 202, 200, [5; 32]);
    // Contract accounts consume a single signature, not the G-account signature map vector.
    if let xdr::SorobanCredentials::Address(c) = &mut valid.credentials {
        let signatures = Vec::<Map<Symbol, Val>>::try_from_val(&env, &c.signature).unwrap();
        let signature = signatures.get(0).unwrap().get(Symbol::new(&env, "signature")).unwrap();
        c.signature = xdr::ScVal::try_from_val(&env, &signature).unwrap();
    }
    let mut bad = valid.clone();
    if let xdr::SorobanCredentials::Address(c) = &mut bad.credentials {
        let mut bytes = BytesN::<64>::try_from_val(&env, &c.signature).unwrap().to_array();
        bytes[0] ^= 1;
        c.signature =
            xdr::ScVal::try_from_val(&env, &BytesN::from_array(&env, &bytes).to_val()).unwrap();
    }
    let before = state(&env, &manager.address);
    set_claim_auths(&env, &manager.address, &signer, &[&key], &checks, &[bad]);
    assert_eq!(manager.try_claim(&signer, &checks), Err(Ok(auth::native_auth_error())));
    assert_diagnostic(&env, xdr::ScError::Auth(ScErrorCode::InvalidAction));
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &manager.address), before);
    set_claim_auths(&env, &manager.address, &signer, &[&key], &checks, &[valid]);
    manager.claim(&signer, &checks);
    assert_eq!(manager.users_claims(&signer, &checks.get(0).unwrap().currency), u(&env, 10));
}

#[test]
fn prepared_real_g_authorization_obeys_revoked_role_and_changed_quorum() {
    for revoke in [true, false] {
        let env = auth::test_env();
        let key = SigningKey::from_bytes(&[55; 32]);
        let signer = account(&env, &key, None, 1);
        let (manager, checks) = setup(&env, &signer);
        let access = FuulAccessControlClient::new(&env, &manager.address);
        let admin = access.get_role_member(&access.default_admin_role(), &0);
        let role = Symbol::new(&env, "claim_signer");
        let prepared = signed_entry(
            &env,
            &signer,
            &[&key],
            invocation(&manager.address, claim_payload(&env, &checks.get(0).unwrap())),
            203,
            200,
            [5; 32],
        );
        if revoke {
            auth::authorize(
                &env,
                &manager.address,
                &admin,
                "revoke_role",
                (&role, &signer, &admin).into_val(&env),
            );
            access.revoke_role(&role, &signer, &admin);
        } else {
            auth::authorize(
                &env,
                &manager.address,
                &admin,
                "set_required_signers",
                (&admin, 2_u128).into_val(&env),
            );
            manager.set_required_signers(&admin, &2);
        }
        let before = state(&env, &manager.address);
        set_claim_auths(
            &env,
            &manager.address,
            &signer,
            &[&key],
            &checks,
            core::slice::from_ref(&prepared),
        );
        assert_eq!(
            manager.try_claim(&signer, &checks),
            Err(Ok(Error::from_contract_error(if revoke { 6307 } else { 6306 })))
        );
        assert!(env.events().all().events().is_empty());
        assert_eq!(state(&env, &manager.address), before);
        assert_eq!(
            MockProjectClient::new(&env, &checks.get(0).unwrap().project_address).last_recipient(),
            None
        );
        if revoke {
            auth::authorize(
                &env,
                &manager.address,
                &admin,
                "grant_role",
                (&role, &signer, &admin).into_val(&env),
            );
            access.grant_role(&role, &signer, &admin);
        } else {
            auth::authorize(
                &env,
                &manager.address,
                &admin,
                "set_required_signers",
                (&admin, 1_u128).into_val(&env),
            );
            manager.set_required_signers(&admin, &1);
        }
        set_claim_auths(&env, &manager.address, &signer, &[&key], &checks, &[prepared]);
        manager.claim(&signer, &checks);
        assert_eq!(manager.users_claims(&signer, &checks.get(0).unwrap().currency), u(&env, 10));
    }
}

#[contract]
struct ClaimProxy;
#[contractimpl]
impl ClaimProxy {
    pub fn execute(e: Env, manager: Address, caller: Address, checks: Vec<ClaimCheck>) {
        FuulManagerClient::new(&e, &manager).claim(&caller, &checks);
    }
}

#[test]
fn another_contract_invoker_cannot_impersonate_the_authorized_invoker() {
    let env = auth::test_env();
    let authorized = env.register(ClaimInvoker, ());
    let wrong = env.register(ClaimProxy, ());
    let (manager, checks) = setup(&env, &authorized);
    let before = state(&env, &manager.address);
    env.set_auths(&[]);
    assert_eq!(
        ClaimProxyClient::new(&env, &wrong).try_execute(&manager.address, &authorized, &checks),
        Err(Ok(auth::native_auth_error()))
    );
    assert_diagnostic(&env, xdr::ScError::Auth(ScErrorCode::InvalidAction));
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &manager.address), before);
    ClaimInvokerClient::new(&env, &authorized).execute(&manager.address, &checks);
    assert_eq!(manager.users_claims(&authorized, &checks.get(0).unwrap().currency), u(&env, 10));
}

#[test]
fn source_account_credentials_use_only_the_trusted_simulated_transaction_source() {
    let env = auth::test_env();
    let key = SigningKey::from_bytes(&[56; 32]);
    let signer = account(&env, &key, None, 1);
    let (manager, checks) = setup(&env, &signer);
    let entry = SorobanAuthorizationEntry {
        credentials: xdr::SorobanCredentials::SourceAccount,
        root_invocation: invocation(&manager.address, claim_payload(&env, &checks.get(0).unwrap())),
    };
    let caller = SorobanAuthorizationEntry {
        credentials: xdr::SorobanCredentials::SourceAccount,
        root_invocation: invocation(&manager.address, (&signer, &checks).into_val(&env)),
    };
    let other = xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(
        SigningKey::from_bytes(&[57; 32]).verifying_key().to_bytes(),
    )));
    // This test injects the host's trusted source identity; it is not a signed transaction E2E.
    env.host().set_source_account(other).unwrap();
    let before = state(&env, &manager.address);
    env.set_auths(&[caller.clone(), entry.clone()]);
    assert_eq!(manager.try_claim(&signer, &checks), Err(Ok(auth::native_auth_error())));
    assert_diagnostic(&env, xdr::ScError::Auth(ScErrorCode::InvalidAction));
    assert!(env.events().all().events().is_empty());
    assert_eq!(state(&env, &manager.address), before);
    let xdr::ScAddress::Account(id) = xdr::ScAddress::from(&signer) else { unreachable!() };
    env.host().set_source_account(id).unwrap();
    env.set_auths(&[caller, entry]);
    manager.claim(&signer, &checks);
    assert_eq!(manager.users_claims(&signer, &checks.get(0).unwrap().currency), u(&env, 10));
}
