use crate::test::*;
use fuul_core::{INSTANCE_EXTEND_AMOUNT as EXTEND, INSTANCE_TTL_THRESHOLD as THRESHOLD};
use soroban_sdk::testutils::{storage::Persistent, Deployer};
use stellar_access::access_control::AccessControlStorageKey;

#[test]
fn maintenance_and_proof_reads_renew_only_their_entries_at_the_threshold() {
    for guest in [false, true] {
        for ttl in [THRESHOLD - 1, THRESHOLD, THRESHOLD + 1] {
            let e = Env::default();
            e.ledger().with_mut(|l| l.min_persistent_entry_ttl = EXTEND + 1);
            let f = guest::fixture(&e, guest, 0, 0);
            let id = f.client.address.clone();
            let p = proof(&e, 81);
            let key = (Symbol::new(&e, "ClaimedProof"), &p);
            let absent = (Symbol::new(&e, "ClaimedProof"), proof(&e, 82));
            guest::authorize(
                &e,
                &id,
                &f.manager,
                "claim",
                (
                    &f.manager,
                    &f.recipient,
                    &f.currency,
                    TokenType::StellarAsset,
                    0_i128,
                    u(&e, 0),
                    &p,
                    false,
                )
                    .into_val(&e),
            );
            f.client.claim(
                &f.manager,
                &f.recipient,
                &f.currency,
                &TokenType::StellarAsset,
                &0,
                &u(&e, 0),
                &p,
                &false,
            );
            let current = e.deployer().get_contract_instance_ttl(&id);
            e.ledger().with_mut(|l| l.sequence_number += current - ttl);
            let role =
                AccessControlStorageKey::HasRole(f.admin.clone(), Symbol::new(&e, "default_admin"));
            let role_before = e.as_contract(&id, || e.storage().persistent().get_ttl(&role));
            let proof_before = e.as_contract(&id, || e.storage().persistent().get_ttl(&key));
            e.mock_auths(&[]);
            f.client.keep_alive();
            let expected = if ttl <= THRESHOLD { EXTEND } else { ttl };
            assert_eq!(e.deployer().get_contract_instance_ttl(&id), expected);
            assert_eq!(e.deployer().get_contract_code_ttl(&id), expected);
            assert_eq!(e.as_contract(&id, || e.storage().persistent().get_ttl(&role)), role_before);
            assert_eq!(e.as_contract(&id, || e.storage().persistent().get_ttl(&key)), proof_before);
            assert!(f.client.claimed_proofs(&p));
            assert_eq!(
                e.as_contract(&id, || e.storage().persistent().get_ttl(&key)),
                if proof_before <= THRESHOLD { EXTEND } else { proof_before }
            );
            assert!(!f.client.claimed_proofs(&proof(&e, 82)));
            assert!(!e.as_contract(&id, || e.storage().persistent().has(&absent)));
        }
    }
}
