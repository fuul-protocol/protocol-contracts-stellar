use crate::test::*;

#[test]
fn token_callback_cannot_reenter_project_removal() {
    use soroban_sdk::{
        xdr::{ScErrorCode, ScErrorType},
        Error,
    };
    let env = Env::default();
    let fixture = project_fixture(&env, false, 0, 0, 0);
    let receiver = Address::generate(&env);
    let currency =
        env.register(ReentrantProjectToken, (fixture.client.address.clone(), receiver.clone()));

    let before = env.to_ledger_snapshot().ledger_entries;
    assert_eq!(
        fixture.client.try_remove_funds(
            &fixture.admin,
            &receiver,
            &currency,
            &TokenType::StellarAsset,
            &1,
            &Vec::new(&env),
            &Vec::new(&env),
        ),
        Err(Ok(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)))
    );
    assert_eq!(env.to_ledger_snapshot().ledger_entries, before);
    assert_eq!(env.events().all().filter_by_contract(&fixture.client.address), std::vec![]);
}
