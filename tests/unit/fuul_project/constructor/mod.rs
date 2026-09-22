use crate::test::*;

#[test]
fn constructor_stores_project_configuration() {
    let env = Env::default();
    let fixture = fixture(&env);

    assert_eq!(fixture.client.factory(), fixture.factory);
    assert_eq!(fixture.client.project_info_uri(), String::from_str(&env, "ipfs://fuul-project"));
    assert!(!fixture.client.kyc_required());
}

#[test]
fn constructor_sets_the_project_admin() {
    let env = Env::default();
    let fixture = fixture(&env);
    let access = FuulAccessControlClient::new(&env, &fixture.client.address);
    assert_eq!(access.get_role_members(&access.default_admin_role()), vec![&env, fixture.admin]);
    assert_eq!(
        env.as_contract(&fixture.client.address, || stellar_access::access_control::get_admin(
            &env
        )),
        None
    );
}

#[test]
fn constructor_publishes_configuration_events() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let factory = Address::generate(&env);
    let uri = String::from_str(&env, "ipfs://fuul-project");
    let contract = env.register(FuulProject, (factory.clone(), admin.clone(), uri.clone(), false));

    assert_eq!(
        env.events().all(),
        std::vec![
            stellar_access::access_control::RoleGranted {
                role: Symbol::new(&env, "default_admin"),
                account: admin,
                caller: factory,
            }
            .to_xdr(&env, &contract),
            ProjectInfoUpdated { project_info_uri: uri }.to_xdr(&env, &contract),
            KycRequiredUpdated { required: false }.to_xdr(&env, &contract),
        ]
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #6100)")]
fn constructor_rejects_an_empty_uri() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let factory = Address::generate(&env);

    env.register(FuulProject, (factory, admin, String::from_str(&env, ""), false));
}

#[test]
#[should_panic(expected = "Error(Auth, InvalidAction)")]
fn project_constructor_rejects_a_spoofed_factory_association() {
    let env = Env::default();
    let factory = env.register(FailingFactory, ());

    env.register(
        FuulProject,
        (factory, Address::generate(&env), String::from_str(&env, "ipfs://spoofed-factory"), false),
    );
}
