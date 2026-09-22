use crate::test::*;

#[contract]
pub(super) struct MockFactory;

#[contractimpl]
impl MockFactory {
    pub fn __constructor(e: &Env, manager: Address, fees: FeesInformation) {
        e.storage().instance().set(&symbol_short!("manager"), &manager);
        e.storage().instance().set(&symbol_short!("fees"), &fees);
    }

    pub fn has_manager_role(e: &Env, account: Address) -> bool {
        e.storage().instance().get::<_, Address>(&symbol_short!("manager")) == Some(account)
    }

    pub fn get_fees_information(e: &Env, _project: Address) -> FeesInformation {
        if e.storage().instance().get(&symbol_short!("fail_fee")).unwrap_or(false) {
            panic_with_error!(e, FailingFactoryError::FeeLookupFailed);
        }
        e.storage().instance().get(&symbol_short!("fees")).expect("fees must be configured")
    }
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) enum FailingFactoryError {
    FeeLookupFailed = 1,
}

#[contract]
pub(super) struct FailingFactory;

#[contractimpl]
impl FailingFactory {
    pub fn get_fees_information(e: &Env, _project: Address) -> FeesInformation {
        panic_with_error!(e, FailingFactoryError::FeeLookupFailed)
    }
}

#[contract]
pub(super) struct MockNonFungible;

#[contractimpl]
impl MockNonFungible {
    pub fn mint(e: &Env, to: Address, token_id: u32) {
        NonFungibleBase::mint(e, &to, token_id);
    }
}

#[contractimpl(contracttrait)]
impl NonFungibleToken for MockNonFungible {
    type ContractType = NonFungibleBase;
}

pub(super) fn multi_token_balance(e: &Env, owner: &Address, token_id: u32) -> i128 {
    e.storage().persistent().get(&(symbol_short!("balance"), owner.clone(), token_id)).unwrap_or(0)
}

pub(super) fn set_multi_token_balance(e: &Env, owner: &Address, token_id: u32, amount: i128) {
    e.storage().persistent().set(&(symbol_short!("balance"), owner.clone(), token_id), &amount);
}

#[contract]
pub(super) struct MockMultiToken;

#[contractimpl]
impl MockMultiToken {
    pub fn mint(e: &Env, to: Address, token_id: u32, amount: i128) {
        assert!(amount >= 0, "amount must not be negative");
        let balance =
            multi_token_balance(e, &to, token_id).checked_add(amount).expect("balance overflow");
        set_multi_token_balance(e, &to, token_id, balance);
    }

    pub fn balance(e: &Env, owner: Address, token_id: u32) -> i128 {
        multi_token_balance(e, &owner, token_id)
    }

    pub fn transfer(e: &Env, from: Address, to: Address, token_id: u32, amount: i128) {
        from.require_auth();
        assert!(amount >= 0, "amount must not be negative");
        let from_balance = multi_token_balance(e, &from, token_id);
        assert!(from_balance >= amount, "insufficient balance");
        if from == to {
            return;
        }
        let to_balance =
            multi_token_balance(e, &to, token_id).checked_add(amount).expect("balance overflow");
        set_multi_token_balance(e, &from, token_id, from_balance - amount);
        set_multi_token_balance(e, &to, token_id, to_balance);
    }
}

#[contract]
pub(super) struct ReentrantProjectToken;

#[contractimpl]
impl ReentrantProjectToken {
    pub fn __constructor(e: &Env, target: Address, receiver: Address) {
        e.storage().instance().set(&symbol_short!("target"), &target);
        e.storage().instance().set(&symbol_short!("receiver"), &receiver);
    }

    pub fn transfer(e: &Env, from: Address, _to: MuxedAddress, _amount: i128) {
        from.require_auth();
        let target: Address =
            e.storage().instance().get(&symbol_short!("target")).expect("target must be set");
        let receiver: Address =
            e.storage().instance().get(&symbol_short!("receiver")).expect("receiver must be set");
        FuulProjectClient::new(e, &target).remove_funds(
            &e.current_contract_address(),
            &receiver,
            &e.current_contract_address(),
            &TokenType::StellarAsset,
            &1,
            &Vec::new(e),
            &Vec::new(e),
        );
    }
}
