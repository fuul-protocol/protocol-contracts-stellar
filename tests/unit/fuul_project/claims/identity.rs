use crate::test::*;
use soroban_sdk::Error;

#[contract]
struct Forwarder;

#[contractimpl]
impl Forwarder {
    pub fn claim(
        e: Env,
        project: Address,
        manager: Address,
        currency: Address,
        to: Address,
        proof: BytesN<32>,
    ) {
        FuulProjectClient::new(&e, &project).claim(
            &manager,
            &to,
            &currency,
            &TokenType::StellarAsset,
            &0,
            &u(&e, 0),
            &proof,
            &false,
        );
    }
}

#[test]
fn contract_invoker_identity_cannot_be_replaced_by_an_argument() {
    for compiled in [false, true] {
        let e = Env::default();
        let f = guest::fixture(&e, compiled, 0, 0);
        let authorized = e.register(Forwarder, ());
        let attacker = e.register(Forwarder, ());
        let factory = f.client.factory();
        e.as_contract(&factory, || {
            e.storage().instance().set(&symbol_short!("manager"), &authorized)
        });
        e.mock_auths(&[]);
        let p = proof(&e, 97);
        let before = e.to_ledger_snapshot().ledger_entries;
        let unauthorized = ForwarderClient::new(&e, &attacker);
        assert!(unauthorized
            .try_claim(&f.client.address, &authorized, &f.currency, &f.recipient, &p)
            .is_err());
        assert_eq!(e.to_ledger_snapshot().ledger_entries, before);
        assert!(e.events().all().events().is_empty());
        assert_eq!(
            unauthorized.try_claim(&f.client.address, &attacker, &f.currency, &f.recipient, &p),
            Err(Ok(Error::from_contract_error(6101)))
        );
        assert!(!f.client.claimed_proofs(&p));
        ForwarderClient::new(&e, &authorized).claim(
            &f.client.address,
            &authorized,
            &f.currency,
            &f.recipient,
            &p,
        );
        assert!(f.client.claimed_proofs(&p));
    }
}
