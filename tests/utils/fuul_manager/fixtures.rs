use crate::test::*;

pub(super) const PROJECT_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_project.wasm"
));

pub(super) struct Fixture<'a> {
    pub(super) client: FuulManagerClient<'a>,
    pub(super) admin: Address,
    pub(super) pauser: Address,
    pub(super) unpauser: Address,
    pub(super) signer: Address,
    pub(super) accepted_currency: Address,
    pub(super) native_asset: Address,
    pub(super) validator: Address,
}

pub(super) struct ClaimFixture<'a> {
    pub(super) client: FuulManagerClient<'a>,
    pub(super) admin: Address,
    pub(super) pauser: Address,
    pub(super) signer: Address,
    pub(super) caller: Address,
    pub(super) recipient: Address,
    pub(super) currency: Address,
    pub(super) native_asset: Address,
}

pub(super) fn register_manager<'a>(
    env: &'a Env,
    required_signers: u32,
    signers: Vec<Address>,
    accepted_currency: &Address,
    native_asset: &Address,
    validator: Option<Address>,
) -> (FuulManagerClient<'a>, Address, Address, Address) {
    let admin = Address::generate(env);
    let pauser = Address::generate(env);
    let unpauser = Address::generate(env);
    let contract = env.register(
        FuulManager,
        (
            admin.clone(),
            pauser.clone(),
            unpauser.clone(),
            u128::from(required_signers),
            signers,
            accepted_currency.clone(),
            native_asset.clone(),
            validator,
            u(env, 1_000_000_000_000_i128),
        ),
    );

    (FuulManagerClient::new(env, &contract), admin, pauser, unpauser)
}

pub(super) fn fixture(env: &Env) -> Fixture<'_> {
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000_000);
    let signer = Address::generate(env);
    let accepted_currency = Address::generate(env);
    let native_asset = Address::generate(env);
    let validator = Address::generate(env);
    let (client, admin, pauser, unpauser) = register_manager(
        env,
        1,
        vec![env, signer.clone()],
        &accepted_currency,
        &native_asset,
        Some(validator.clone()),
    );

    Fixture { client, admin, pauser, unpauser, signer, accepted_currency, native_asset, validator }
}

pub(super) fn claim_fixture(env: &Env) -> ClaimFixture<'_> {
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000_000);
    let signer = Address::generate(env);
    let caller = Address::generate(env);
    let recipient = Address::generate(env);
    let currency_admin = Address::generate(env);
    let native_admin = Address::generate(env);
    let currency = env.register_stellar_asset_contract_v2(currency_admin).address();
    let native_asset = env.register_stellar_asset_contract_v2(native_admin).address();
    let (client, admin, pauser, _) =
        register_manager(env, 1, vec![env, signer.clone()], &currency, &native_asset, None);
    env.mock_all_auths();
    StellarAssetClient::new(env, &native_asset).mint(&caller, &1_000_000);

    ClaimFixture { client, admin, pauser, signer, caller, recipient, currency, native_asset }
}

pub(super) fn register_mock_project<'a>(
    env: &'a Env,
    collector: &Address,
    native_user_claim_fee: i128,
) -> MockProjectClient<'a> {
    let address = env.register(
        MockProject,
        (ProjectClaimResult { native_user_claim_fee, fee_collector: collector.clone() },),
    );
    MockProjectClient::new(env, &address)
}

pub(super) fn claim_check(
    env: &Env,
    fixture: &ClaimFixture<'_>,
    project: &Address,
    amount: i128,
    proof_value: u8,
) -> ClaimCheck {
    ClaimCheck {
        project_address: project.clone(),
        to: fixture.recipient.clone(),
        currency: fixture.currency.clone(),
        currency_type: TokenType::StellarAsset,
        amount,
        reason: ClaimReason::AffiliatePayout,
        token_id: u(env, 0),
        deadline: u(env, env.ledger().timestamp() + 300),
        proof: BytesN::from_array(env, &[proof_value; 32]),
        signers: vec![env, fixture.signer.clone()],
    }
}

pub(super) fn smart_account_claim_authorization(
    env: &Env,
    account: &Address,
    manager: &Address,
    caller: &Address,
    checks: &Vec<ClaimCheck>,
) -> std::vec::Vec<SorobanAuthorizationEntry> {
    let caller_invoke = MockAuthInvoke {
        contract: manager,
        fn_name: "claim",
        args: (caller, checks).into_val(env),
        sub_invokes: &[],
    };
    let caller_auth = MockAuth { address: caller, invoke: &caller_invoke };
    env.mock_auths(core::slice::from_ref(&caller_auth));
    assert_eq!(checks.len(), 1);
    let invoke = MockAuthInvoke {
        contract: manager,
        fn_name: "claim",
        args: (checks.get(0).unwrap().authorization(),).into_val(env),
        sub_invokes: &[],
    };
    std::vec![caller_auth.into(), MockAuth { address: account, invoke: &invoke }.into()]
}

pub(super) fn assert_role(env: &Env, fixture: &Fixture<'_>, account: &Address, role: &Symbol) {
    let access = FuulAccessControlClient::new(env, &fixture.client.address);
    assert!(access.has_role(role, account));
}
