use crate::test::{constructor_helpers::*, roles_pause_helpers::test_env, *};

#[test]
fn bootstrap_allows_overlapping_admin_operational_and_signer_accounts() {
    for shared_admin in [false, true] {
        let env = test_env();
        let mut input = Bootstrap::new(&env);
        input.unpauser = input.pauser.clone();
        input.signers = vec![&env, input.pauser.clone()];
        if shared_admin {
            input.admin = input.pauser.clone();
        }
        let id = input.register(&env);
        assert_eq!(env.events().all(), expected_events(&env, &id, &input));
        assert_bootstrap(&env, &id, &input);
        let access = FuulAccessControlClient::new(&env, &id);
        assert_eq!(
            access.get_role_members(&access.default_admin_role()),
            vec![&env, input.admin.clone()]
        );
        for role in ["pauser", "unpauser", "claim_signer"] {
            assert!(access.has_role(&Symbol::new(&env, role), &input.pauser));
        }
    }
}

#[test]
fn bootstrap_signer_enumeration_is_exact_at_quorum_one_and_count() {
    for quorum in [1, 3] {
        let env = test_env();
        let mut input = Bootstrap::new(&env);
        input.signers.push_back(Address::generate(&env));
        input.signers.push_back(Address::generate(&env));
        input.quorum = quorum;
        let id = input.register(&env);
        assert_bootstrap(&env, &id, &input);
        let access = FuulAccessControlClient::new(&env, &id);
        let role = Symbol::new(&env, "claim_signer");
        assert_eq!(access.get_role_member_count(&role), 3);
        for (index, signer) in input.signers.iter().enumerate() {
            assert!(access.has_role(&role, &signer));
            assert_eq!(access.get_role_member(&role, &(index as u32)), signer);
        }
        assert_eq!(FuulManagerClient::new(&env, &id).required_signers(), quorum);
    }
}
