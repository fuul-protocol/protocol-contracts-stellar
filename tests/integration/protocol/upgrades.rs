use crate::test::*;
use fuul_core::access::FuulAccessControlClient;
use fuul_core::upgrade::{ContractUpgraded, UpgradeableClient};
use soroban_sdk::{testutils::Deployer as _, xdr};

const MANAGER_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_manager.wasm"
));
const FACTORY_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_factory.wasm"
));
const UPGRADED_MANAGER: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_upgrade_manager_fixture.wasm"
));
const UPGRADED_FACTORY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_upgrade_factory_fixture.wasm"
));
const UPGRADED_PROJECT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_upgrade_project_fixture.wasm"
));

struct UpgradeFixture<'a> {
    manager: FuulManagerClient<'a>,
    factory: FuulFactoryClient<'a>,
    project: FuulProjectClient<'a>,
    admins: [Address; 3],
    recipient: Address,
    collector: Address,
    caller: Address,
    currency: Address,
    native: Address,
    check: ClaimCheck,
}

impl UpgradeFixture<'_> {
    fn contracts(&self) -> [Address; 3] {
        [self.manager.address.clone(), self.factory.address.clone(), self.project.address.clone()]
    }
}

/// Project code is protocol-controlled: its upgrade operator is a Factory administrator.
fn upgrade_admin<'a>(f: &'a UpgradeFixture<'_>, index: usize) -> &'a Address {
    &f.admins[if index == 2 { 1 } else { index }]
}

/// The contract whose `default_admin` role grants upgrade authority for contract `index`.
fn upgrade_authority(f: &UpgradeFixture<'_>, index: usize) -> Address {
    if index == 2 {
        f.factory.address.clone()
    } else {
        f.contracts()[index].clone()
    }
}

/// OpenZeppelin `Unauthorized` (2000) for own-role checks; Project `Unauthorized` (6101) for the Factory-role check.
fn unauthorized_code(index: usize) -> u32 {
    if index == 2 {
        6101
    } else {
        2000
    }
}

fn setup(e: &Env, guest: bool) -> UpgradeFixture<'_> {
    e.ledger().with_mut(|l| {
        l.sequence_number = 100;
        l.timestamp = 1_000_000;
    });
    e.mock_all_auths();
    let admins = [Address::generate(e), Address::generate(e), Address::generate(e)];
    let currency = e.register_stellar_asset_contract_v2(admins[0].clone()).address();
    let native = e.register_stellar_asset_contract_v2(admins[0].clone()).address();
    let signer = Address::generate(e);
    let args = (
        &admins[0],
        &admins[0],
        &admins[0],
        1_u128,
        vec![e, signer.clone()],
        &currency,
        &native,
        None::<Address>,
        u(e, 1_000_000),
    );
    let manager_id =
        if guest { e.register(MANAGER_WASM, args) } else { e.register(FuulManager, args) };
    let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
    let collector = Address::generate(e);
    let args = (&admins[1], &manager_id, &collector, &code);
    let factory_id =
        if guest { e.register(FACTORY_WASM, args) } else { e.register(FuulFactory, args) };
    let manager = FuulManagerClient::new(e, &manager_id);
    let factory = FuulFactoryClient::new(e, &factory_id);
    let project_id = factory.create_fuul_project(
        &admins[2],
        &String::from_str(e, "ipfs://upgrade-state"),
        &false,
    );
    let project = FuulProjectClient::new(e, &project_id);
    factory.set_project_claim_fee(&admins[1], &project_id, &250);
    factory.set_native_user_claim_fee(&admins[1], &project_id, &17);
    let recipient = Address::generate(e);
    let caller = Address::generate(e);
    StellarAssetClient::new(e, &currency).mint(&project_id, &100_000);
    StellarAssetClient::new(e, &native).mint(&caller, &100);
    let check = ClaimCheck {
        project_address: project_id,
        to: recipient.clone(),
        currency: currency.clone(),
        currency_type: TokenType::StellarAsset,
        amount: 1_000,
        reason: ClaimReason::AffiliatePayout,
        token_id: u(e, 0),
        deadline: u(e, 1_000_600),
        proof: BytesN::from_array(e, &[71; 32]),
        signers: vec![e, signer],
    };
    manager.claim(&caller, &vec![e, check.clone()]);
    UpgradeFixture {
        manager,
        factory,
        project,
        admins,
        recipient,
        collector,
        caller,
        currency,
        native,
        check,
    }
}

// Read every logical value for exactly this contract. Exclude the executable
// identity and ledger metadata, which must change during an upgrade.
fn storage_values(
    e: &Env,
    id: &Address,
) -> std::vec::Vec<(xdr::ContractDataDurability, xdr::ScVal, xdr::ScVal)> {
    let address: xdr::ScAddress = id.clone().into();
    let mut values = std::vec::Vec::new();
    for (_, entry) in e.host().get_stored_entries().unwrap() {
        let Some((entry, _)) = entry else { continue };
        let xdr::LedgerEntryData::ContractData(data) = &entry.data else { continue };
        if data.contract != address {
            continue;
        }
        let value = match &data.val {
            xdr::ScVal::ContractInstance(instance) => xdr::ScVal::Map(instance.storage.clone()),
            value => value.clone(),
        };
        values.push((data.durability, data.key.clone(), value));
    }
    values.sort();
    values
}

fn authorize(e: &Env, id: &Address, operator: &Address, hash: &BytesN<32>) {
    e.mock_auths(&[MockAuth {
        address: operator,
        invoke: &MockAuthInvoke {
            contract: id,
            fn_name: "upgrade",
            args: (hash, operator).into_val(e),
            sub_invokes: &[],
        },
    }]);
}

#[test]
fn upgrades_require_exact_operator_authorization_and_a_current_admin_role() {
    for guest in [false, true] {
        let e = Env::default();
        let f = setup(&e, guest);
        // Iteration 1 hands the Factory role to a replacement; the Project iteration must use it.
        let mut factory_admin = f.admins[1].clone();
        for (index, id) in f.contracts().iter().enumerate() {
            let admin = &(if index == 2 { factory_admin.clone() } else { f.admins[index].clone() });
            let client = UpgradeableClient::new(&e, id);
            let hash = e.deployer().upload_contract_wasm(
                [UPGRADED_MANAGER, UPGRADED_FACTORY, UPGRADED_PROJECT][index],
            );
            let before = storage_values(&e, id);
            e.mock_auths(&[]);
            assert!(client.try_upgrade(&hash, admin).is_err());
            let outsider = &f.admins[(index + 1) % 3];
            authorize(&e, id, outsider, &hash);
            assert_eq!(
                client.try_upgrade(&hash, outsider),
                Err(Ok(Error::from_contract_error(unauthorized_code(index))))
            );
            authorize(&e, id, admin, &BytesN::from_array(&e, &[0; 32]));
            assert!(client.try_upgrade(&hash, admin).is_err());
            assert_eq!(storage_values(&e, id), before);
            assert!(e.events().all().events().is_empty());

            e.mock_all_auths();
            let access = FuulAccessControlClient::new(&e, &upgrade_authority(&f, index));
            let role = Symbol::new(&e, "default_admin");
            let replacement = Address::generate(&e);
            access.grant_role(&role, &replacement, admin);
            access.revoke_role(&role, admin, &replacement);
            authorize(&e, id, admin, &hash);
            assert_eq!(
                client.try_upgrade(&hash, admin),
                Err(Ok(Error::from_contract_error(unauthorized_code(index))))
            );
            authorize(&e, id, &replacement, &hash);
            client.upgrade(&hash, &replacement);
            assert!(access.has_role(&role, &replacement));
            assert!(!access.has_role(&role, admin));
            if index == 1 {
                factory_admin = replacement;
            }
        }
    }
}

#[test]
fn failed_upgrade_preserves_all_contract_values_and_emits_no_upgrade_event() {
    for guest in [false, true] {
        let e = Env::default();
        let f = setup(&e, guest);
        for (index, id) in f.contracts().iter().enumerate() {
            let before = storage_values(&e, id);
            let ttl = e.deployer().get_contract_instance_ttl(id);
            let absent = BytesN::from_array(&e, &[99; 32]);
            authorize(&e, id, upgrade_admin(&f, index), &absent);
            assert!(UpgradeableClient::new(&e, id)
                .try_upgrade(&absent, upgrade_admin(&f, index))
                .is_err());
            assert_eq!(storage_values(&e, id), before);
            assert_eq!(e.deployer().get_contract_instance_ttl(id), ttl);
            assert!(e.events().all().events().is_empty());
        }
        assert!(f.project.claimed_proofs(&f.check.proof));
        assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.recipient), 1_000);
    }
}

#[test]
fn actual_guest_upgrades_extend_new_code_ttl_preserve_state_and_keep_claims_safe() {
    let e = Env::default();
    let f = setup(&e, true);
    f.manager.pause(&f.admins[0]);
    for (index, id) in f.contracts().iter().enumerate() {
        let before = storage_values(&e, id);
        let hash = e
            .deployer()
            .upload_contract_wasm([UPGRADED_MANAGER, UPGRADED_FACTORY, UPGRADED_PROJECT][index]);
        authorize(&e, id, upgrade_admin(&f, index), &hash);
        UpgradeableClient::new(&e, id).upgrade(&hash, upgrade_admin(&f, index));
        assert_eq!(
            e.events().all().filter_by_contract(id),
            std::vec![ContractUpgraded {
                operator: upgrade_admin(&f, index).clone(),
                new_wasm_hash: hash
            }
            .to_xdr(&e, id)]
        );
        assert!(e.deployer().get_contract_code_ttl(id) >= fuul_core::INSTANCE_EXTEND_AMOUNT - 1);
        assert_eq!(storage_values(&e, id), before);
        assert_eq!(
            e.invoke_contract::<u32>(id, &Symbol::new(&e, "fixture_schema_version"), Vec::new(&e)),
            0
        );
        let args: Vec<Val> = (&f.admins[index],).into_val(&e);
        e.mock_auths(&[]);
        assert!(e
            .try_invoke_contract::<(), Error>(id, &Symbol::new(&e, "migrate_fixture"), args.clone())
            .is_err());
        e.mock_all_auths();
        e.invoke_contract::<()>(id, &Symbol::new(&e, "migrate_fixture"), args.clone());
        assert_eq!(
            e.invoke_contract::<u32>(id, &Symbol::new(&e, "fixture_schema_version"), Vec::new(&e)),
            2
        );
        assert_eq!(
            e.try_invoke_contract::<(), Error>(id, &Symbol::new(&e, "migrate_fixture"), args),
            Err(Ok(Error::from_contract_error(6900)))
        );
    }
    assert!(f.manager.paused());
    assert_eq!(f.factory.contract_tracker(), 1);
    assert_eq!(f.project.factory(), f.factory.address);
    assert_eq!(f.project.project_info_uri(), String::from_str(&e, "ipfs://upgrade-state"));
    assert!(f.project.claimed_proofs(&f.check.proof));
    assert_eq!(f.manager.users_claims(&f.recipient, &f.currency), u(&e, 1_000));
    assert_eq!(f.manager.currency_limits(&f.currency).cumulative_claim_per_cooldown, u(&e, 1_000));
    e.mock_all_auths();
    f.manager.unpause(&f.admins[0]);
    assert_eq!(
        f.manager.try_claim(&f.caller, &vec![&e, f.check.clone()]),
        Err(Ok(Error::from_contract_error(6102)))
    );
    let next = ClaimCheck { proof: BytesN::from_array(&e, &[72; 32]), ..f.check.clone() };
    f.manager.claim(&f.caller, &vec![&e, next]);
    assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.recipient), 2_000);
    assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.collector), 50);
    assert_eq!(TokenClient::new(&e, &f.currency).balance(&f.project.address), 97_950);
    assert_eq!(TokenClient::new(&e, &f.native).balance(&f.caller), 66);
    assert_eq!(TokenClient::new(&e, &f.native).balance(&f.collector), 34);
    assert_eq!(f.manager.users_claims(&f.recipient, &f.currency), u(&e, 2_000));
    let next_project = f.factory.create_fuul_project(
        &f.admins[2],
        &String::from_str(&e, "ipfs://after-upgrade"),
        &false,
    );
    assert_ne!(next_project, f.project.address);
    assert_eq!(f.factory.contract_tracker(), 2);
}

/// Audit 2026-09-20, C-1: the Manager charges the claim caller the native fee and collector that
/// the Project returns, so a Project administrator must never be able to replace Project code.
#[test]
fn project_administrator_cannot_replace_project_code() {
    for guest in [false, true] {
        let e = Env::default();
        let f = setup(&e, guest);
        let project_admin = &f.admins[2];
        let factory_admin = &f.admins[1];
        let before = storage_values(&e, &f.project.address);
        let hash = e.deployer().upload_contract_wasm(UPGRADED_PROJECT);
        let client = UpgradeableClient::new(&e, &f.project.address);
        // Holding the Project's own default_admin role grants no upgrade authority.
        assert!(f.project.has_role(&Symbol::new(&e, "default_admin"), project_admin));
        authorize(&e, &f.project.address, project_admin, &hash);
        assert_eq!(
            client.try_upgrade(&hash, project_admin),
            Err(Ok(Error::from_contract_error(6101)))
        );
        assert_eq!(storage_values(&e, &f.project.address), before);
        assert!(e.events().all().filter_by_contract(&f.project.address).events().is_empty());
        // Granting more Project roles does not help either.
        e.mock_all_auths();
        let helper = Address::generate(&e);
        f.project.grant_role(&Symbol::new(&e, "default_admin"), &helper, project_admin);
        let granted = storage_values(&e, &f.project.address);
        authorize(&e, &f.project.address, &helper, &hash);
        assert_eq!(client.try_upgrade(&hash, &helper), Err(Ok(Error::from_contract_error(6101))));
        assert_eq!(storage_values(&e, &f.project.address), granted);
        // A caller still pays exactly the Factory fee through the unchanged Project code.
        e.mock_all_auths();
        let next = ClaimCheck { proof: BytesN::from_array(&e, &[72; 32]), ..f.check.clone() };
        let caller_before = TokenClient::new(&e, &f.native).balance(&f.caller);
        f.manager.claim(&f.caller, &vec![&e, next]);
        assert_eq!(TokenClient::new(&e, &f.native).balance(&f.caller), caller_before - 17);
        // Only a Factory administrator installs new Project code.
        authorize(&e, &f.project.address, factory_admin, &hash);
        client.upgrade(&hash, factory_admin);
        assert_eq!(
            e.events().all().filter_by_contract(&f.project.address),
            std::vec![ContractUpgraded { operator: factory_admin.clone(), new_wasm_hash: hash }
                .to_xdr(&e, &f.project.address)]
        );
    }
}
