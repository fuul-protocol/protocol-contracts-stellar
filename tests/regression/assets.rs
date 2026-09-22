use super::*;
use stellar_tokens::non_fungible::{Base as NonFungibleBase, NonFungibleToken};

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

#[test]
fn nft_and_multi_token_claims_transfer_one_item_and_share_proof_protection() {
    for multi in [false, true] {
        let e = Env::default();
        let f = Fixture::new(&e);
        let currency =
            if multi { e.register(MockMultiToken, ()) } else { e.register(MockNonFungible, ()) };
        if multi {
            MockMultiTokenClient::new(&e, &currency).mint(&f.project.address, &7, &5);
        } else {
            MockNonFungibleClient::new(&e, &currency).mint(&f.project.address, &7);
        }
        let mut c = f.check(50, 0);
        c.currency = currency.clone();
        c.token_id = u(&e, 7);
        c.currency_type = if multi { TokenType::MultiToken } else { TokenType::NonFungible };
        // Non-fungible claims use zero accounting amounts, including for an unset currency limit.
        f.claim(&c);
        if multi {
            let t = MockMultiTokenClient::new(&e, &currency);
            assert_eq!(t.balance(&f.recipient, &7), 1);
            assert_eq!(t.balance(&f.project.address, &7), 4);
        } else {
            assert_eq!(MockNonFungibleClient::new(&e, &currency).owner_of(&7), f.recipient);
        }
        assert_eq!(f.manager.users_claims(&f.recipient, &currency), u(&e, 0));
        assert_eq!(TokenClient::new(&e, &f.native).balance(&f.collector), 20_000);
        assert_eq!(f.manager.try_claim(&f.caller, &vec![&e, c.clone()]), Err(Ok(error(6102))));
        c.proof = BytesN::from_array(&e, &[51; 32]);
        c.token_id = u(&e, u32::MAX as u128 + 1);
        assert_eq!(f.manager.try_claim(&f.caller, &vec![&e, c.clone()]), Err(Ok(error(6105))));
        assert!(!f.project.claimed_proofs(&c.proof));
    }
}

#[test]
fn token_withdrawal_batches_reject_invalid_ids_lengths_and_partial_failure() {
    for multi in [false, true] {
        let e = Env::default();
        let f = Fixture::new(&e);
        let currency =
            if multi { e.register(MockMultiToken, ()) } else { e.register(MockNonFungible, ()) };
        if multi {
            MockMultiTokenClient::new(&e, &currency).mint(&f.project.address, &7, &5);
        } else {
            MockNonFungibleClient::new(&e, &currency).mint(&f.project.address, &7);
        }
        let kind = if multi { TokenType::MultiToken } else { TokenType::NonFungible };
        for ids in [vec![&e, -1], vec![&e, u32::MAX as i128 + 1]] {
            assert_eq!(
                f.project.try_remove_funds(
                    &f.project_admin,
                    &f.recipient,
                    &currency,
                    &kind,
                    &0,
                    &ids,
                    &vec![&e, 1]
                ),
                Err(Ok(error(6105)))
            );
        }
        if multi {
            assert_eq!(
                f.project.try_remove_funds(
                    &f.project_admin,
                    &f.recipient,
                    &currency,
                    &kind,
                    &0,
                    &vec![&e, 7],
                    &vec![&e]
                ),
                Err(Ok(error(6105)))
            );
        }
        assert!(f
            .project
            .try_remove_funds(
                &f.project_admin,
                &f.recipient,
                &currency,
                &kind,
                &0,
                &vec![&e, 7, 8],
                &vec![&e, 1, 1]
            )
            .is_err());
        if multi {
            let t = MockMultiTokenClient::new(&e, &currency);
            assert_eq!(t.balance(&f.project.address, &7), 5);
            assert_eq!(t.balance(&f.recipient, &7), 0);
        } else {
            assert_eq!(MockNonFungibleClient::new(&e, &currency).owner_of(&7), f.project.address);
        }
        f.project.remove_funds(
            &f.project_admin,
            &f.recipient,
            &currency,
            &kind,
            &0,
            &vec![&e, 7],
            &vec![&e, 1],
        );
        if multi {
            assert_eq!(MockMultiTokenClient::new(&e, &currency).balance(&f.recipient, &7), 1);
        } else {
            assert_eq!(MockNonFungibleClient::new(&e, &currency).owner_of(&7), f.recipient);
        }
    }
}
