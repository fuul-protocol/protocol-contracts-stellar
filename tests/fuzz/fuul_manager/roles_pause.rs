use crate::test::{roles_pause_helpers::*, *};
use proptest::prelude::*;

#[derive(Clone, Debug)]
struct Model {
    roles: [[bool; 2]; 4],
    paused: bool,
}

#[test]
fn generated_role_capacity_churn_matches_independent_model() {
    use proptest::test_runner::{Config, TestRunner};
    use stellar_access::access_control as oz;

    for guest in [false, true] {
        let env = test_env();
        env.cost_estimate().budget().reset_unlimited();
        let input = constructor_helpers::Bootstrap::new(&env);
        let id = if guest {
            env.register(constructor_helpers::MANAGER_WASM, input.args(&env))
        } else {
            input.register(&env)
        };
        let access = FuulAccessControlClient::new(&env, &id);
        let members = [Address::generate(&env), Address::generate(&env)];
        let roles =
            [Symbol::new(&env, ""), Symbol::new(&env, "churn_a"), Symbol::new(&env, "churn_b")];
        env.mock_all_auths();
        for index in 4..254 {
            access.grant_role(
                &Symbol::new(&env, &std::format!("fixed_{index}")),
                &input.admin,
                &input.admin,
            );
        }
        let mut runner = TestRunner::new(Config { failure_persistence: None, ..Config::default() });
        let strategy = prop::collection::vec(
            (any::<bool>(), 0usize..3, 0usize..2, any::<bool>(), any::<bool>()),
            1..13,
        );
        // Reuse the fixed 254-type fixture; reset only the six mutable memberships per case.
        runner
            .run(&strategy, |steps| {
                env.mock_all_auths();
                for role in &roles {
                    for member in &members {
                        access.revoke_role(role, member, &input.admin);
                    }
                }
                for role in &roles[..2] {
                    access.grant_role(role, &members[0], &input.admin);
                }
                let mut model = [1_u8, 1, 0];
                for (grant, role, member, signed, admin) in steps {
                    let actor = if admin { &input.admin } else { &members[0] };
                    let name = if grant { "grant_role" } else { "revoke_role" };
                    let args = (&roles[role], &members[member], actor).into_val(&env);
                    if signed {
                        authorize(&env, &id, actor, name, args);
                    } else {
                        env.mock_auths(&[]);
                    }
                    let full = model.iter().filter(|&&mask| mask != 0).count() == 2;
                    let error = if !signed {
                        Some(native_auth_error())
                    } else if !admin {
                        Some(Error::from_contract_error(2000))
                    } else if grant && model[role] == 0 && full {
                        Some(Error::from_contract_error(2010))
                    } else {
                        None
                    };
                    let before = error.map(|_| security_helpers::state(&env, &id));
                    let result = if grant {
                        access.try_grant_role(&roles[role], &members[member], actor)
                    } else {
                        access.try_revoke_role(&roles[role], &members[member], actor)
                    };
                    if let Some(error) = error {
                        prop_assert_eq!(result, Err(Ok(error)));
                        prop_assert!(env.events().all().events().is_empty());
                        prop_assert_eq!(security_helpers::state(&env, &id), before.unwrap());
                    } else {
                        prop_assert_eq!(result, Ok(Ok(())));
                        let old = model[role];
                        if grant {
                            model[role] |= 1 << member;
                        } else {
                            model[role] &= !(1 << member);
                        }
                        prop_assert_eq!(
                            env.events().all().events().len(),
                            usize::from(old != model[role])
                        );
                    }
                    let count = env.as_contract(&id, || oz::get_existing_roles(&env).len());
                    prop_assert_eq!(
                        count,
                        254 + model.iter().filter(|&&mask| mask != 0).count() as u32
                    );
                    for (i, role) in roles.iter().enumerate() {
                        prop_assert_eq!(access.get_role_member_count(role), model[i].count_ones());
                        for (j, account) in members.iter().enumerate() {
                            prop_assert_eq!(
                                access.has_role(role, account),
                                model[i] & (1 << j) != 0
                            );
                        }
                    }
                }
                Ok(())
            })
            .unwrap();
    }
}

// Actor 0 is the fixed admin; role administration is intentionally not modeled.
// Operations: grant, revoke, pause, unpause. A role index selects the target role.
impl Model {
    fn step(
        &mut self,
        op: u8,
        actor: usize,
        target: usize,
        role: usize,
        auth: bool,
    ) -> Option<u32> {
        if !auth {
            return None;
        }
        if op < 2 {
            if actor != 0 {
                return Some(2000);
            }
            self.roles[target][role] = op == 0;
        } else {
            let required_role = usize::from(op == 3);
            if !self.roles[actor][required_role] {
                return Some(2000);
            }
            let next = op == 2;
            if self.paused == next {
                return Some(if next { 1000 } else { 1001 });
            }
            self.paused = next;
        }
        Some(0)
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        ..ProptestConfig::default()
    })]
    #[test]
    fn generated_role_pause_sequences_match_independent_model(steps in prop::collection::vec((0u8..4, 0usize..4, 0usize..4, 0usize..2, any::<bool>()), 1..49)) {
        let env = test_env();
        let f = fixture(&env);
        let accounts = [f.admin.clone(), f.pauser.clone(), f.unpauser.clone(), Address::generate(&env)];
        let roles = [Symbol::new(&env, "pauser"), Symbol::new(&env, "unpauser")];
        let access = FuulAccessControlClient::new(&env, &f.client.address);
        let mut model = Model { roles: [[false, false], [true, false], [false, true], [false, false]], paused: false };
        for (op, actor, target, role, auth) in steps {
            let before = state(&env, &f.client, &accounts);
            let ttl_before = ttl(&env, &f.client);
            let caller = &accounts[actor];
            let account = &accounts[target];
            let role_symbol = &roles[role];
            let name = ["grant_role", "revoke_role", "pause", "unpause"][usize::from(op)];
            let args = if op < 2 { (role_symbol, account, caller).into_val(&env) } else { (caller,).into_val(&env) };
            if auth { authorize(&env, &f.client.address, caller, name, args); } else { env.set_auths(&[]); }
            let diagnostics = diagnostic_count(&env);
            let result = match op {
                0 => access.try_grant_role(role_symbol, account, caller),
                1 => access.try_revoke_role(role_symbol, account, caller),
                2 => f.client.try_pause(caller),
                _ => f.client.try_unpause(caller),
            };
            let expected = model.step(op, actor, target, role, auth);
            let events = env.events().all();
            match expected {
                Some(0) => prop_assert_eq!(result, Ok(Ok(()))),
                Some(code) => prop_assert_eq!(result, Err(Ok(Error::from_contract_error(code)))),
                None => {
                    prop_assert_eq!(result, Err(Ok(native_auth_error())));
                    assert_auth_failure(&env, diagnostics);
                }
            }
            // as_contract used by ttl() resets invocation diagnostics.
            let ttl_after = ttl(&env, &f.client);
            let after = state(&env, &f.client, &accounts);
            if expected != Some(0) {
                prop_assert!(events.events().is_empty());
                prop_assert_eq!(ttl_after, ttl_before);
                prop_assert_eq!(&after, &before);
            }
            prop_assert_eq!(after.0, model.paused);
            prop_assert_eq!(after.1, model.roles.iter().flatten().copied().collect::<std::vec::Vec<_>>());
        }
    }
}
