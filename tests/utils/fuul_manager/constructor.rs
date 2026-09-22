use crate::test::*;
use soroban_sdk::{
    testutils::storage::{Instance, Persistent, Temporary},
    Map,
};
use stellar_access::access_control::{AccessControlStorageKey as OzKey, RoleGranted};

pub(super) const MANAGER_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_manager.wasm"
));

#[derive(Clone)]
pub(super) struct Bootstrap {
    pub admin: Address,
    pub pauser: Address,
    pub unpauser: Address,
    pub quorum: u128,
    pub signers: Vec<Address>,
    pub accepted: Address,
    pub native: Address,
    pub kyc: Option<Address>,
}

impl Bootstrap {
    pub fn new(env: &Env) -> Self {
        Self {
            admin: Address::generate(env),
            pauser: Address::generate(env),
            unpauser: Address::generate(env),
            quorum: 1,
            signers: vec![env, Address::generate(env)],
            accepted: Address::generate(env),
            native: Address::generate(env),
            kyc: None,
        }
    }
    pub fn args(&self, env: &Env) -> Vec<Val> {
        (
            &self.admin,
            &self.pauser,
            &self.unpauser,
            self.quorum,
            &self.signers,
            &self.accepted,
            &self.native,
            &self.kyc,
            u(env, 1_000_000_000_000_i128),
        )
            .into_val(env)
    }
    pub fn register(&self, env: &Env) -> Address {
        env.register(FuulManager, self.args(env))
    }
}

// Encoded contracttype enum keys: inspect the existing schema without exporting DataKey.
pub(super) fn instance_key(env: &Env, name: &str) -> Val {
    (Symbol::new(env, name),).into_val(env)
}
pub(super) fn limit_key(env: &Env, asset: &Address) -> Val {
    (Symbol::new(env, "CurrencyLimit"), asset).into_val(env)
}

// OZ does not re-export the RoleAccountKey payload type; encode its public storage schema.
pub(super) fn role_member_key(env: &Env, role: &Symbol, index: u32) -> Val {
    let fields = Map::<Symbol, Val>::from_array(
        env,
        [
            (Symbol::new(env, "index"), index.into_val(env)),
            (Symbol::new(env, "role"), role.into_val(env)),
        ],
    );
    (Symbol::new(env, "RoleAccounts"), fields).into_val(env)
}

pub(super) fn expected_events(
    env: &Env,
    id: &Address,
    input: &Bootstrap,
) -> std::vec::Vec<soroban_sdk::xdr::ContractEvent> {
    let mut result = std::vec::Vec::new();
    for (name, account) in [
        ("default_admin", input.admin.clone()),
        ("pauser", input.pauser.clone()),
        ("unpauser", input.unpauser.clone()),
    ]
    .into_iter()
    .chain(input.signers.iter().map(|s| ("claim_signer", s)))
    {
        result.push(
            RoleGranted { role: Symbol::new(env, name), account, caller: input.admin.clone() }
                .to_xdr(env, id),
        );
    }
    for token in [&input.accepted, &input.native] {
        result.push(
            TokenLimitAdded { token: token.clone(), limit: u(env, 1_000_000_000_000_i128) }
                .to_xdr(env, id),
        );
    }
    result
}

pub(super) fn assert_bootstrap(env: &Env, id: &Address, input: &Bootstrap) {
    env.as_contract(id, || {
        let mut instance = Map::<Val, Val>::new(env);
        instance.set(instance_key(env, "ClaimCooldown"), 86_400_u128.into_val(env));
        instance.set(instance_key(env, "RequiredSigners"), input.quorum.into_val(env));
        instance.set(instance_key(env, "KycValidator"), input.kyc.into_val(env));
        instance.set(instance_key(env, "NativeAsset"), input.native.into_val(env));
        assert_eq!(env.storage().instance().all(), instance);
        let mut persistent = Map::<Val, Val>::new(env);
        let roles = vec![
            env,
            Symbol::new(env, "default_admin"),
            Symbol::new(env, "pauser"),
            Symbol::new(env, "unpauser"),
            Symbol::new(env, "claim_signer"),
        ];
        persistent.set(OzKey::ExistingRoles.into_val(env), roles.clone().into_val(env));
        for (role, members) in [
            (roles.get(0).unwrap(), vec![env, input.admin.clone()]),
            (roles.get(1).unwrap(), vec![env, input.pauser.clone()]),
            (roles.get(2).unwrap(), vec![env, input.unpauser.clone()]),
            (roles.get(3).unwrap(), input.signers.clone()),
        ] {
            persistent.set(
                OzKey::RoleAccountsCount(role.clone()).into_val(env),
                members.len().into_val(env),
            );
            for (index, account) in members.iter().enumerate() {
                persistent.set(
                    OzKey::HasRole(account.clone(), role.clone()).into_val(env),
                    (index as u32).into_val(env),
                );
                persistent.set(role_member_key(env, &role, index as u32), account.into_val(env));
            }
        }
        let limit = CurrencyTokenLimit {
            claim_limit_per_cooldown: u(env, 1_000_000_000_000_i128),
            cumulative_claim_per_cooldown: u(env, 0),
            claim_cooldown_period_started: env.ledger().timestamp(),
        };
        for asset in [&input.accepted, &input.native] {
            persistent.set(limit_key(env, asset), limit.clone().into_val(env));
        }
        // Exact maps also prove no stored pause flag, exemptions, user claims or pending admin.
        assert_eq!(env.storage().persistent().all(), persistent);
        assert!(env.storage().temporary().all().is_empty());
    });
}

pub(super) fn assert_diagnostic(env: &Env, expected: soroban_sdk::xdr::ScError) {
    use soroban_sdk::xdr::{ContractEventBody, ScVal};
    fn contains(value: &ScVal, expected: &soroban_sdk::xdr::ScError) -> bool {
        match value {
            ScVal::Error(error) => error == expected,
            ScVal::Vec(Some(values)) => values.iter().any(|v| contains(v, expected)),
            _ => false,
        }
    }
    let diagnostics = env.host().get_diagnostic_events().unwrap();
    assert!(
        diagnostics.0.iter().any(|event| {
            let ContractEventBody::V0(body) = &event.event.body;
            body.topics.iter().any(|v| contains(v, &expected)) || contains(&body.data, &expected)
        }),
        "missing typed {expected:?}: {diagnostics:?}"
    );
}

pub(super) fn reject_native(env: &Env, input: &Bootstrap, code: u32) {
    use soroban_sdk::xdr::ScError;
    let id = Address::generate(env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.register_at(&id, FuulManager, input.args(env))
    }));
    assert!(result.is_err(), "invalid constructor unexpectedly succeeded");
    // Both error layers are required; an unrelated panic cannot qualify as expected rejection.
    assert_diagnostic(env, ScError::Context(ScErrorCode::InvalidAction));
    assert_diagnostic(env, ScError::Contract(code));
    assert!(env.events().all().events().is_empty());
    // register_at installs test metadata before invoking the native constructor.
    // Only its business storage is expected to roll back here.
    env.as_contract(&id, || {
        assert!(env.storage().instance().all().is_empty());
        assert!(env.storage().persistent().all().is_empty());
        assert!(env.storage().temporary().all().is_empty());
    });
}
