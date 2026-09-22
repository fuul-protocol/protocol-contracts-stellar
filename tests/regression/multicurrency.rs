use super::*;

#[test]
fn one_factory_supports_distinct_project_currencies_and_shared_per_currency_limits() {
    let e = Env::default();
    let f = Fixture::new(&e);
    let second_currency = e.register_stellar_asset_contract_v2(f.admin.clone()).address();
    f.manager.add_currency_limit(&f.admin, &second_currency, &u(&e, 500));
    let second_project = FuulProjectClient::new(
        &e,
        &f.factory.create_fuul_project(
            &f.project_admin,
            &String::from_str(&e, "ipfs://second-project"),
            &false,
        ),
    );
    let mint = StellarAssetClient::new(&e, &second_currency);
    mint.mint(&second_project.address, &1_000);
    mint.mint(&f.project.address, &1_000);

    let first = f.check(101, 100);
    let mut second = f.check(102, 200);
    second.project_address = second_project.address.clone();
    second.currency = second_currency.clone();
    f.manager.claim(&f.caller, &vec![&e, first, second.clone()]);
    assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.recipient), 100);
    let token = TokenClient::new(&e, &second_currency);
    assert_eq!(token.balance(&f.recipient), 200);
    assert_eq!(token.balance(&second_project.address), 798);
    assert_eq!(token.balance(&f.project.address), 1_000);
    assert_eq!(second_project.factory(), f.factory.address);

    // The first Project can also pay the second currency; its constructor fixes no asset.
    let mut third = f.check(103, 300);
    third.currency = second_currency.clone();
    f.claim(&third);
    assert_eq!(token.balance(&f.recipient), 500);
    assert_eq!(token.balance(&f.project.address), 697);
    assert_eq!(token.balance(&f.collector), 5);
    assert_eq!(f.manager.users_claims(&f.recipient, &second_currency), u(&e, 500));
    assert_eq!(f.manager.users_claims(&f.recipient, &f.currency), u(&e, 100));
    assert_eq!(
        f.manager.currency_limits(&second_currency).cumulative_claim_per_cooldown,
        u(&e, 500)
    );

    // A currency's Manager limit is shared by Projects, not reset for each Project.
    second.amount = 1;
    second.proof = BytesN::from_array(&e, &[104; 32]);
    assert_eq!(f.manager.try_claim(&f.caller, &vec![&e, second.clone()]), Err(Ok(error(6304))));
    assert!(!second_project.claimed_proofs(&second.proof));
    assert_eq!(token.balance(&f.recipient), 500);
    assert_eq!(token.balance(&second_project.address), 798);
}
