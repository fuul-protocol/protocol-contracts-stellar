extern crate std;

use fuul_core::{ClaimCheck, ClaimReason, TokenType};
use fuul_factory::{FuulFactory, FuulFactoryClient};
use fuul_project::FuulProjectClient;
use proptest::prelude::*;
use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig, Ledger, MockAuth, MockAuthInvoke},
    token::{StellarAssetClient, TokenClient},
    vec, Address, BytesN, Env, Error, IntoVal, MuxedAddress, String, Vec,
};

use crate::{FuulManager, FuulManagerClient};

const PROJECT_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_project.wasm"
));
const MANAGER_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_manager.wasm"
));
const FACTORY_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/fuul_factory.wasm"
));
const COOLDOWN: u64 = 86_400;
const NATIVE_FEE: i128 = 20_000;

#[derive(Clone, Debug)]
pub(crate) struct Claim {
    proof: u8,
    amount: u16,
    recipient: bool,
    expired: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum Action {
    Claim(std::vec::Vec<Claim>, u8),
    Advance(u64),
    Pause(bool),
    Exempt(bool),
    Fee(u16),
    Limit(u16),
    AddLimit(u16),
    Fund(u16),
    Remove(u16),
}

pub(crate) fn actions() -> impl Strategy<Value = Action> {
    let expired = prop_oneof![7 => Just(false), 1 => Just(true)];
    let claim = (0u8..16, 0u16..5_001, any::<bool>(), expired).prop_map(
        |(proof, amount, recipient, expired)| Claim { proof, amount, recipient, expired },
    );
    prop_oneof![
        8 => (proptest::collection::vec(claim, 1..4), prop_oneof![7 => Just(0), 1 => 1u8..4])
            .prop_map(|(claims, auth)| Action::Claim(claims, auth)),
        2 => prop::sample::select(std::vec![0, 1, COOLDOWN - 1, COOLDOWN, COOLDOWN + 1])
            .prop_map(Action::Advance),
        1 => any::<bool>().prop_map(Action::Pause),
        1 => any::<bool>().prop_map(Action::Exempt),
        1 => prop::sample::select(std::vec![0, 1, 100, 9_999, 10_000]).prop_map(Action::Fee),
        1 => prop_oneof![Just(0), 1u16..10_001].prop_map(Action::Limit),
        1 => (0u16..10_001).prop_map(Action::AddLimit),
        2 => (0u16..10_001).prop_map(Action::Fund),
        1 => (0u16..10_001).prop_map(Action::Remove),
    ]
}

#[derive(Clone, Debug)]
struct Model {
    time: u64,
    period_start: u64,
    period_total: i128,
    limit: i128,
    project: i128,
    recipients: [i128; 2],
    collector: i128,
    removed: i128,
    minted: i128,
    payer_native: i128,
    collector_native: i128,
    native_minted: i128,
    proofs: [bool; 16],
    paused: bool,
    exempt: bool,
    fee_bps: i128,
}

impl Model {
    fn new() -> Self {
        Self {
            time: 1_000_000,
            period_start: 1_000_000,
            period_total: 0,
            limit: 5_000,
            project: 20_000,
            recipients: [0; 2],
            collector: 0,
            removed: 0,
            minted: 20_000,
            payer_native: 100_000,
            collector_native: 0,
            native_minted: 100_000,
            proofs: [false; 16],
            paused: false,
            exempt: false,
            fee_bps: 100,
        }
    }

    // This accounting oracle uses only business inputs and integer balances.
    // It never calls the contract's math, storage or authorization helpers.
    fn claim(&self, claims: &[Claim], auth: u8) -> Option<Self> {
        if self.paused || auth != 0 {
            return None;
        }
        let mut next = self.clone();
        let payout = claims.iter().map(|claim| i128::from(claim.amount)).sum::<i128>();
        if self.time - self.period_start >= COOLDOWN {
            next.period_start = self.time;
            next.period_total = 0;
        }
        if next.period_total + payout > self.limit {
            return None;
        }
        for claim in claims {
            let amount = i128::from(claim.amount);
            let fee = amount * self.fee_bps / 10_000;
            if claim.expired || next.proofs[usize::from(claim.proof)] || next.project < amount + fee
            {
                return None;
            }
            next.project -= amount + fee;
            next.collector += fee;
            next.recipients[usize::from(claim.recipient)] += amount;
            next.proofs[usize::from(claim.proof)] = true;
        }
        let native_fee = if self.exempt { 0 } else { NATIVE_FEE * claims.len() as i128 };
        if next.payer_native < native_fee {
            return None;
        }
        next.payer_native -= native_fee;
        next.collector_native += native_fee;
        next.period_total += payout;
        Some(next)
    }
}

pub(crate) fn exercise(sequence: &[Action], compiled: bool) {
    let env = Env::new_with_config(EnvTestConfig { capture_snapshot_at_drop: false });
    env.ledger().with_mut(|ledger| ledger.timestamp = 1_000_000);
    let admin = Address::generate(&env);
    let signer = Address::generate(&env);
    let caller = Address::generate(&env);
    let recipients = [Address::generate(&env), Address::generate(&env)];
    let collector = Address::generate(&env);
    let removal_receiver = Address::generate(&env);
    let currency = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let native = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let manager_args = (
        admin.clone(),
        admin.clone(),
        admin.clone(),
        1u128,
        vec![&env, signer.clone()],
        currency.clone(),
        native.clone(),
        None::<Address>,
        crate::test::u(&env, 1_000_000_000_000_i128),
    );
    let manager_id = if compiled {
        env.register(MANAGER_WASM, manager_args)
    } else {
        env.register(FuulManager, manager_args)
    };
    let manager = FuulManagerClient::new(&env, &manager_id);
    let project_hash = env.deployer().upload_contract_wasm(PROJECT_WASM);
    let factory_args = (admin.clone(), manager_id.clone(), collector.clone(), project_hash);
    let factory_id = if compiled {
        env.register(FACTORY_WASM, factory_args)
    } else {
        env.register(FuulFactory, factory_args)
    };
    let factory = FuulFactoryClient::new(&env, &factory_id);
    let project_id =
        factory.create_fuul_project(&admin, &String::from_str(&env, "ipfs://model"), &false);
    let project = FuulProjectClient::new(&env, &project_id);
    let mut model = Model::new();
    env.mock_all_auths();
    manager.set_currency_token_limit(&admin, &currency, &crate::test::u(&env, model.limit));
    StellarAssetClient::new(&env, &currency).mint(&project_id, &model.minted);
    StellarAssetClient::new(&env, &native).mint(&caller, &model.native_minted);

    for action in sequence {
        // Administrative operations use known authorized roles. Claims below
        // replace this with exact caller/signer trees, including missing auth.
        env.mock_all_auths();
        match action {
            Action::Claim(claims, auth) => {
                let mut checks = Vec::new(&env);
                let mut authorizations = Vec::new(&env);
                for claim in claims {
                    let check = ClaimCheck {
                        project_address: project_id.clone(),
                        to: recipients[usize::from(claim.recipient)].clone(),
                        currency: currency.clone(),
                        currency_type: TokenType::StellarAsset,
                        amount: i128::from(claim.amount),
                        reason: ClaimReason::EndUserPayout,
                        token_id: crate::test::u(&env, 0),
                        deadline: crate::test::u(
                            &env,
                            if claim.expired { model.time - 1 } else { model.time },
                        ),
                        proof: BytesN::from_array(&env, &[claim.proof; 32]),
                        signers: vec![&env, signer.clone()],
                    };
                    let mut authorized = check.authorization();
                    if *auth == 3 {
                        authorized.amount += 1;
                    }
                    authorizations.push_back(authorized);
                    checks.push_back(check);
                }
                let transfers: std::vec::Vec<_> = if model.exempt {
                    std::vec![]
                } else {
                    std::vec![MockAuthInvoke {
                        contract: &native,
                        fn_name: "transfer",
                        args: (
                            &caller,
                            MuxedAddress::from(&collector),
                            NATIVE_FEE * claims.len() as i128
                        )
                            .into_val(&env),
                        sub_invokes: &[],
                    }]
                };
                let caller_auth = MockAuth {
                    address: &caller,
                    invoke: &MockAuthInvoke {
                        contract: &manager_id,
                        fn_name: "claim",
                        args: (&caller, &checks).into_val(&env),
                        sub_invokes: &transfers,
                    },
                };
                let signer_roots: std::vec::Vec<_> = authorizations
                    .iter()
                    .map(|authorization| MockAuthInvoke {
                        contract: &manager_id,
                        fn_name: "claim",
                        args: (authorization,).into_val(&env),
                        sub_invokes: &[],
                    })
                    .collect();
                let mut entries = std::vec::Vec::new();
                if *auth != 2 {
                    entries.push(caller_auth);
                }
                if *auth != 1 {
                    entries.extend(
                        signer_roots.iter().map(|root| MockAuth { address: &signer, invoke: root }),
                    );
                }
                env.mock_auths(&entries);
                let expected = model.claim(claims, *auth);
                let result = manager.try_claim(&caller, &checks);
                assert_eq!(
                    result.is_ok(),
                    expected.is_some(),
                    "action: {action:?}; model: {model:?}; result: {result:?}"
                );
                if let Some(next) = expected {
                    model = next;
                }
            }
            Action::Advance(seconds) => {
                model.time += seconds;
                env.ledger().with_mut(|ledger| ledger.timestamp = model.time);
            }
            Action::Pause(paused) => {
                if model.paused != *paused {
                    if *paused {
                        manager.pause(&admin);
                    } else {
                        manager.unpause(&admin);
                    }
                    model.paused = *paused;
                }
            }
            Action::Exempt(exempt) => {
                if model.exempt != *exempt {
                    if *exempt {
                        manager.add_no_claim_fee_address(&admin, &caller);
                    } else {
                        manager.remove_no_claim_fee_address(&admin, &caller);
                    }
                    model.exempt = *exempt;
                }
            }
            Action::Fee(bps) => {
                if model.fee_bps != i128::from(*bps) {
                    factory.set_project_claim_fee(&admin, &project_id, &u32::from(*bps));
                    model.fee_bps = i128::from(*bps);
                }
            }
            Action::Limit(limit) => {
                let limit = i128::from(*limit);
                let accepted = model.limit > 0 && limit != model.limit;
                assert_eq!(
                    manager.try_set_currency_token_limit(
                        &admin,
                        &currency,
                        &crate::test::u(&env, limit)
                    ),
                    if accepted { Ok(Ok(())) } else { Err(Ok(Error::from_contract_error(6300))) }
                );
                if accepted {
                    model.limit = limit;
                }
            }
            Action::AddLimit(limit) => {
                let limit = i128::from(*limit);
                let accepted = model.limit == 0 && limit > 0;
                assert_eq!(
                    manager.try_add_currency_limit(&admin, &currency, &crate::test::u(&env, limit)),
                    if accepted {
                        Ok(Ok(()))
                    } else {
                        Err(Ok(Error::from_contract_error(if limit == 0 { 6300 } else { 6302 })))
                    }
                );
                if accepted {
                    model.limit = limit;
                    model.period_total = 0;
                    model.period_start = model.time;
                }
            }
            Action::Fund(amount) => {
                let amount = i128::from(*amount);
                StellarAssetClient::new(&env, &currency).mint(&project_id, &amount);
                StellarAssetClient::new(&env, &native).mint(&caller, &NATIVE_FEE);
                model.project += amount;
                model.minted += amount;
                model.payer_native += NATIVE_FEE;
                model.native_minted += NATIVE_FEE;
            }
            Action::Remove(amount) => {
                let amount = i128::from(*amount);
                let accepted = amount >= 0 && amount <= model.project;
                let result = project.try_remove_funds(
                    &admin,
                    &removal_receiver,
                    &currency,
                    &TokenType::StellarAsset,
                    &amount,
                    &Vec::new(&env),
                    &Vec::new(&env),
                );
                assert_eq!(result.is_ok(), accepted);
                if accepted {
                    model.project -= amount;
                    model.removed += amount;
                }
            }
        }

        let token = TokenClient::new(&env, &currency);
        let native_token = TokenClient::new(&env, &native);
        assert_eq!(token.balance(&project_id), model.project);
        assert_eq!(token.balance(&collector), model.collector);
        assert_eq!(token.balance(&removal_receiver), model.removed);
        assert_eq!(native_token.balance(&caller), model.payer_native);
        assert_eq!(native_token.balance(&collector), model.collector_native);
        for (index, recipient) in recipients.iter().enumerate() {
            assert_eq!(token.balance(recipient), model.recipients[index]);
            assert_eq!(
                manager.users_claims(recipient, &currency),
                crate::test::u(&env, model.recipients[index])
            );
        }
        assert_eq!(
            model.project + model.collector + model.removed + model.recipients.iter().sum::<i128>(),
            model.minted
        );
        assert_eq!(model.payer_native + model.collector_native, model.native_minted);
        let limit = manager.currency_limits(&currency);
        assert_eq!(limit.claim_limit_per_cooldown, crate::test::u(&env, model.limit));
        assert_eq!(limit.cumulative_claim_per_cooldown, crate::test::u(&env, model.period_total));
        assert_eq!(limit.claim_cooldown_period_started, model.period_start);
        assert_eq!(manager.paused(), model.paused);
        assert_eq!(manager.no_claim_fee_addresses(&caller), model.exempt);
        assert_eq!(i128::from(factory.project_fees(&project_id).project_claim_fee), model.fee_bps);
        for (proof, claimed) in model.proofs.iter().enumerate() {
            assert_eq!(
                project.claimed_proofs(&BytesN::from_array(&env, &[proof as u8; 32])),
                *claimed
            );
        }
    }
}

#[test]
fn per_claim_fee_rounding_regression_matches_accounting_model() {
    let sequence = [
        Action::Fee(9_999),
        Action::Claim(
            std::vec![
                Claim { proof: 0, amount: 2, recipient: false, expired: false },
                Claim { proof: 1, amount: 2, recipient: false, expired: false },
            ],
            0,
        ),
    ];
    for compiled in [false, true] {
        exercise(&sequence, compiled);
    }
}

pub(crate) fn model_exercises_success_replay_batch_rollback_and_cooldown_boundary() {
    let claim = Claim { proof: 0, amount: 1_000, recipient: false, expired: false };
    let sequence = [
        Action::Claim(std::vec![claim.clone()], 0),
        Action::Claim(std::vec![claim.clone()], 0),
        Action::Claim(std::vec![Claim { proof: 1, ..claim.clone() }, claim.clone()], 0),
        Action::Claim(std::vec![Claim { proof: 1, ..claim.clone() }], 1),
        Action::Claim(std::vec![Claim { proof: 1, ..claim.clone() }], 2),
        Action::Claim(std::vec![Claim { proof: 1, ..claim.clone() }], 3),
        Action::Advance(COOLDOWN),
        Action::Claim(std::vec![Claim { proof: 1, ..claim.clone() }], 0),
        Action::Limit(500),
        Action::Claim(std::vec![Claim { proof: 2, amount: 100, ..claim.clone() }], 0),
        Action::Advance(COOLDOWN),
        Action::Claim(std::vec![Claim { proof: 2, amount: 100, ..claim.clone() }], 0),
        Action::Limit(0),
        Action::Limit(500),
        Action::AddLimit(500),
        Action::Claim(
            std::vec![
                Claim { proof: 3, amount: 100, ..claim.clone() },
                Claim { proof: 4, amount: 100, ..claim }
            ],
            0,
        ),
    ];
    exercise(&sequence, false);
    exercise(&sequence, true);
}
