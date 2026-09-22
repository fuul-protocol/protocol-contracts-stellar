use crate::test::*;

pub(super) struct Fixture<'a> {
    pub(super) client: FuulProjectClient<'a>,
    pub(super) admin: Address,
    pub(super) factory: Address,
}

pub(super) struct ClaimFixture<'a> {
    pub(super) client: FuulProjectClient<'a>,
    pub(super) admin: Address,
    pub(super) manager: Address,
    pub(super) recipient: Address,
    pub(super) collector: Address,
    pub(super) currency: Address,
    pub(super) token_admin: Address,
}

pub(super) fn fixture(env: &Env) -> Fixture<'_> {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let factory = Address::generate(env);
    let uri = String::from_str(env, "ipfs://fuul-project");
    let contract = env.register(FuulProject, (factory.clone(), admin.clone(), uri, false));
    env.set_auths(&[]);
    let client = FuulProjectClient::new(env, &contract);

    Fixture { client, admin, factory }
}

pub(super) fn claim_fixture(
    env: &Env,
    kyc_required: bool,
    project_fee_bps: u32,
    native_user_claim_fee: i128,
) -> ClaimFixture<'_> {
    project_fixture(env, kyc_required, project_fee_bps, native_user_claim_fee, 0)
}

pub(super) fn project_fixture(
    env: &Env,
    kyc_required: bool,
    project_fee_bps: u32,
    native_user_claim_fee: i128,
    remove_fee_bps: u32,
) -> ClaimFixture<'_> {
    configured_project_fixture(
        env,
        false,
        kyc_required,
        ProjectFees {
            native_user_claim_fee,
            project_claim_fee: project_fee_bps,
            remove_fee: remove_fee_bps,
        },
        1_000_000,
    )
}

pub(super) fn configured_project_fixture(
    env: &Env,
    guest: bool,
    kyc_required: bool,
    fees: ProjectFees,
    funding: i128,
) -> ClaimFixture<'_> {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let manager = Address::generate(env);
    let recipient = Address::generate(env);
    let collector = Address::generate(env);
    let token_admin = Address::generate(env);
    let currency = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    let fees = FeesInformation { fee_collector: collector.clone(), fees };
    let factory = env.register(MockFactory, (manager.clone(), fees));
    let uri = String::from_str(env, if guest { "ipfs://guest" } else { "ipfs://claim-project" });
    let args = (factory, admin.clone(), uri, kyc_required);
    let project = if guest {
        env.mock_all_auths_allowing_non_root_auth();
        env.register(crate::test::guest::PROJECT_WASM, args)
    } else {
        env.register(FuulProject, args)
    };
    StellarAssetClient::new(env, &currency).mint(&project, &funding);

    ClaimFixture {
        client: FuulProjectClient::new(env, &project),
        admin,
        manager,
        recipient,
        collector,
        currency,
        token_admin,
    }
}

pub(super) fn proof(env: &Env, value: u8) -> BytesN<32> {
    BytesN::from_array(env, &[value; 32])
}
