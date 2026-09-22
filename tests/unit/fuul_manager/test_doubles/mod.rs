use crate::test::{roles_pause_helpers as auth, security_helpers::state, *};

const TOKEN_ID: u32 = 7;

fn authorize_transfer(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) {
    auth::authorize(env, token, from, "transfer", (from, to, TOKEN_ID, amount).into_val(env));
}

fn assert_panic_cause(env: &Env, since: usize, message: &str) {
    let diagnostics = env.host().get_diagnostic_events().unwrap();
    assert!(
        diagnostics.0.iter().skip(since).any(|event| {
            event.failed_call && std::format!("{:?}", event.event).contains(message)
        }),
        "missing panic cause {message}: {diagnostics:?}"
    );
}

#[test]
fn multi_token_self_transfer_preserves_balance_for_zero_partial_and_full_amounts() {
    for amount in [1, 0, 10] {
        let env = auth::test_env();
        let token = env.register(MockMultiToken, ());
        let client = MockMultiTokenClient::new(&env, &token);
        let owner = Address::generate(&env);
        client.mint(&owner, &TOKEN_ID, &10);
        let before = state(&env, &token);
        authorize_transfer(&env, &token, &owner, &owner, amount);

        client.transfer(&owner, &owner, &TOKEN_ID, &amount);

        assert_eq!(state(&env, &token), before, "self-transfer amount {amount}");
        assert_eq!(client.balance(&owner, &TOKEN_ID), 10);
    }
}

#[test]
fn multi_token_max_balance_self_transfer_does_not_overflow() {
    for amount in [1, i128::MAX] {
        let env = auth::test_env();
        let token = env.register(MockMultiToken, ());
        let client = MockMultiTokenClient::new(&env, &token);
        let owner = Address::generate(&env);
        client.mint(&owner, &TOKEN_ID, &i128::MAX);
        let before = state(&env, &token);
        authorize_transfer(&env, &token, &owner, &owner, amount);

        assert_eq!(
            client.try_transfer(&owner, &owner, &TOKEN_ID, &amount),
            Ok(Ok(())),
            "self-transfer amount {amount}: {:?}",
            env.host().get_diagnostic_events().unwrap()
        );

        assert_eq!(state(&env, &token), before);
        assert_eq!(client.balance(&owner, &TOKEN_ID), i128::MAX);
    }
}

#[test]
fn multi_token_self_transfer_keeps_auth_amount_and_balance_guards() {
    for (authorized, amount, cause) in [
        (false, 1, "auth"),
        (false, -1, "auth"),
        (true, -1, "amount must not be negative"),
        (true, 11, "insufficient balance"),
    ] {
        let env = auth::test_env();
        let token = env.register(MockMultiToken, ());
        let client = MockMultiTokenClient::new(&env, &token);
        let owner = Address::generate(&env);
        client.mint(&owner, &TOKEN_ID, &10);
        let before = state(&env, &token);
        if authorized {
            authorize_transfer(&env, &token, &owner, &owner, amount);
        } else {
            env.set_auths(&[]);
        }
        let since = auth::diagnostic_count(&env);

        assert_eq!(
            client.try_transfer(&owner, &owner, &TOKEN_ID, &amount),
            Err(Ok(auth::native_auth_error()))
        );
        if authorized {
            assert_panic_cause(&env, since, cause);
        } else {
            auth::assert_auth_failure(&env, since);
        }
        assert_eq!(state(&env, &token), before);
        assert_eq!(client.balance(&owner, &TOKEN_ID), 10);
    }
}

#[test]
fn multi_token_distinct_transfer_conserves_balances_and_overflow_is_atomic() {
    for (receiver_balance, amount, overflow) in [(3, 4, false), (i128::MAX, 1, true)] {
        let env = auth::test_env();
        let token = env.register(MockMultiToken, ());
        let client = MockMultiTokenClient::new(&env, &token);
        let from = Address::generate(&env);
        let to = Address::generate(&env);
        client.mint(&from, &TOKEN_ID, &10);
        client.mint(&to, &TOKEN_ID, &receiver_balance);
        client.mint(&from, &(TOKEN_ID + 1), &99);
        let before = state(&env, &token);
        authorize_transfer(&env, &token, &from, &to, amount);
        let since = auth::diagnostic_count(&env);

        let result = client.try_transfer(&from, &to, &TOKEN_ID, &amount);

        if overflow {
            assert_eq!(result, Err(Ok(auth::native_auth_error())));
            assert_panic_cause(&env, since, "balance overflow");
            assert_eq!(state(&env, &token), before);
            assert_eq!(client.balance(&from, &TOKEN_ID), 10);
            assert_eq!(client.balance(&to, &TOKEN_ID), receiver_balance);
        } else {
            assert_eq!(result, Ok(Ok(())));
            assert_eq!(client.balance(&from, &TOKEN_ID), 6);
            assert_eq!(client.balance(&to, &TOKEN_ID), 7);
            assert_eq!(client.balance(&from, &TOKEN_ID) + client.balance(&to, &TOKEN_ID), 13);
        }
        assert_eq!(client.balance(&from, &(TOKEN_ID + 1)), 99);
    }
}
