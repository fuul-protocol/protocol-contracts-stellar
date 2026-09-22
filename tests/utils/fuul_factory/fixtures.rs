use crate::test::*;

pub(super) const PROJECT_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_project.wasm"
));

pub(super) struct Fixture<'a> {
    pub(super) client: FuulFactoryClient<'a>,
    pub(super) admin: Address,
    pub(super) manager: Address,
    pub(super) collector: Address,
    pub(super) project_admin: Address,
    pub(super) project_wasm_hash: BytesN<32>,
}

pub(super) fn fixture(env: &Env) -> Fixture<'_> {
    let admin = Address::generate(env);
    let manager = Address::generate(env);
    let collector = Address::generate(env);
    let project_admin = Address::generate(env);
    let project_wasm_hash = env.deployer().upload_contract_wasm(PROJECT_WASM);
    let contract = env.register(
        FuulFactory,
        (admin.clone(), manager.clone(), collector.clone(), project_wasm_hash.clone()),
    );
    let client = FuulFactoryClient::new(env, &contract);

    Fixture { client, admin, manager, collector, project_admin, project_wasm_hash }
}

pub(super) fn create_fuul_project(fixture: &Fixture<'_>, env: &Env, uri: &str) -> Address {
    fixture.client.create_fuul_project(&fixture.project_admin, &String::from_str(env, uri), &false)
}
