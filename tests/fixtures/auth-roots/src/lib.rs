#![no_std]

use soroban_sdk::{
    contract, contractevent, contractimpl, symbol_short, Address, Env, IntoVal, Map, Symbol, Val,
    Vec,
};

// Register separate claim/payment instances of this test-only Wasm.
#[contract]
pub struct RootProbe;

#[contractevent]
pub struct ProbeCompleted {
    pub count: u32,
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
            RootProbeClient::new(&e, &payment).transfer(&caller, &collector, &10);
        }
    }

    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        let balance: i128 = e.storage().persistent().get(&from).unwrap_or(100);
        assert!(amount >= 0 && balance >= amount);
        e.storage().persistent().set(&from, &(balance - amount));
        let received: i128 = e.storage().persistent().get(&to).unwrap_or(0);
        e.storage().persistent().set(&to, &(received + amount));
    }
}
