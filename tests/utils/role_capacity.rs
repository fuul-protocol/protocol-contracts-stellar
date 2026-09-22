extern crate std;

use fuul_core::access::FuulAccessControlClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger, MockAuth, MockAuthInvoke},
    xdr::{LedgerEntry, LedgerKey, ScAddress, ScErrorCode, ScErrorType},
    Address, Env, Error, IntoVal, Symbol, Val, Vec,
};
use stellar_access::access_control::{self as oz, RoleGranted, RoleRevoked};

fn state(e: &Env, id: &Address) -> std::vec::Vec<(LedgerKey, LedgerEntry, Option<u32>)> {
    let owner: ScAddress = id.into();
    let mut entries: std::vec::Vec<_> = e
        .to_ledger_snapshot()
        .ledger_entries
        .into_iter()
        .filter_map(|(key, (entry, ttl))| {
            matches!(key.as_ref(), LedgerKey::ContractData(data) if data.contract == owner)
                .then_some((*key, *entry, ttl))
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

fn authorize(e: &Env, id: &Address, actor: &Address, name: &str, args: Vec<Val>) {
    e.mock_auths(&[MockAuth {
        address: actor,
        invoke: &MockAuthInvoke { contract: id, fn_name: name, args, sub_invokes: &[] },
    }]);
}

pub fn prepare(e: &Env) {
    e.cost_estimate().budget().reset_unlimited();
    e.ledger().with_mut(|l| l.min_persistent_entry_ttl = 90 * 17_280);
}

pub fn assert_capacity(e: &Env, id: &Address, admin: &Address, bootstrap_roles: u32) {
    use soroban_sdk::Event;
    let access = FuulAccessControlClient::new(e, id);
    assert_eq!(e.as_contract(id, || oz::get_existing_roles(e).len()), bootstrap_roles);
    let member = Address::generate(e);
    e.mock_all_auths();
    for index in bootstrap_roles..256 {
        access.grant_role(&Symbol::new(e, &std::format!("capacity_{index}")), &member, admin);
    }
    assert_eq!(e.as_contract(id, || oz::get_existing_roles(e).len()), 256);
    let existing = Symbol::new(e, &std::format!("capacity_{bootstrap_roles}"));
    let next = Symbol::new(e, "next_type");
    e.ledger().with_mut(|l| l.sequence_number += 70 * 17_280);

    // Authorization must precede both the duplicate shortcut and the cap error.
    for role in [&existing, &next] {
        for (actor, signer, code) in [
            (admin, None, None),
            (admin, Some(&member), None),
            (&member, Some(&member), Some(2000)),
        ] {
            if let Some(signer) = signer {
                authorize(e, id, signer, "grant_role", (role, &member, actor).into_val(e));
            } else {
                e.mock_auths(&[]);
            }
            let before = state(e, id);
            let result = access.try_grant_role(role, &member, actor);
            assert_eq!(
                result,
                Err(Ok(code.map_or_else(
                    || Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction),
                    Error::from_contract_error,
                )))
            );
            assert!(e.events().all().events().is_empty());
            assert_eq!(state(e, id), before);
        }
    }
    authorize(e, id, admin, "grant_role", (&next, &member, admin).into_val(e));
    let before = state(e, id);
    assert_eq!(
        access.try_grant_role(&next, &member, admin),
        Err(Ok(Error::from_contract_error(2010)))
    );
    assert!(e.events().all().events().is_empty());
    assert_eq!(state(e, id), before);

    // Members do not consume role-type capacity, even past 256 accounts.
    let mut members = std::vec![member.clone()];
    for _ in 1..257 {
        let account = Address::generate(e);
        authorize(e, id, admin, "grant_role", (&existing, &account, admin).into_val(e));
        access.grant_role(&existing, &account, admin);
        assert_eq!(
            e.events().all(),
            std::vec![RoleGranted {
                role: existing.clone(),
                account: account.clone(),
                caller: admin.clone()
            }
            .to_xdr(e, id)]
        );
        members.push(account);
    }
    assert_eq!(access.get_role_member_count(&existing), 257);
    assert_eq!(e.as_contract(id, || oz::get_existing_roles(e).len()), 256);
    authorize(e, id, admin, "grant_role", (&existing, &member, admin).into_val(e));
    access.grant_role(&existing, &member, admin);
    assert!(e.events().all().events().is_empty());
    assert_eq!(access.get_role_member_count(&existing), 257);

    authorize(e, id, admin, "revoke_role", (&existing, &members[1], admin).into_val(e));
    access.revoke_role(&existing, &members[1], admin);
    assert_eq!(
        e.events().all(),
        std::vec![RoleRevoked {
            role: existing.clone(),
            account: members[1].clone(),
            caller: admin.clone()
        }
        .to_xdr(e, id)]
    );
    assert_eq!(access.get_role_member(&existing, &1), members[256]);
    e.as_contract(id, || {
        assert_eq!(oz::has_role(e, &members[256], &existing), Some(1));
        assert_eq!(oz::get_role_member(e, &existing, 1), members[256]);
        assert_eq!(oz::has_role(e, &members[1], &existing), None);
    });
    let vacant = Symbol::new(e, &std::format!("capacity_{}", bootstrap_roles + 1));
    authorize(e, id, admin, "revoke_role", (&vacant, &member, admin).into_val(e));
    access.revoke_role(&vacant, &member, admin);
    assert_eq!(
        e.events().all(),
        std::vec![RoleRevoked {
            role: vacant.clone(),
            account: member.clone(),
            caller: admin.clone()
        }
        .to_xdr(e, id)]
    );
    assert_eq!(access.get_role_member_count(&vacant), 0);
    assert_eq!(e.as_contract(id, || oz::get_existing_roles(e).len()), 255);
    authorize(e, id, admin, "grant_role", (&next, &member, admin).into_val(e));
    access.grant_role(&next, &member, admin);
    assert!(access.has_role(&next, &member));
    authorize(e, id, admin, "revoke_role", (&next, &member, admin).into_val(e));
    access.revoke_role(&next, &member, admin);
    authorize(e, id, admin, "grant_role", (&vacant, &member, admin).into_val(e));
    access.grant_role(&vacant, &member, admin);
    assert_eq!(access.get_role_member(&vacant, &0), member);
    assert_eq!(e.as_contract(id, || oz::get_existing_roles(e).len()), 256);

    let admin_role = access.default_admin_role();
    let second_admin = Address::generate(e);
    authorize(e, id, admin, "grant_role", (&admin_role, &second_admin, admin).into_val(e));
    access.grant_role(&admin_role, &second_admin, admin);
    assert_eq!(access.get_role_member_count(&admin_role), 2);
    for actor in [admin, &second_admin] {
        authorize(e, id, actor, "grant_role", (&existing, &member, actor).into_val(e));
        access.grant_role(&existing, &member, actor);
        assert!(e.events().all().events().is_empty());
    }
    authorize(e, id, admin, "revoke_role", (&admin_role, admin, admin).into_val(e));
    access.revoke_role(&admin_role, admin, admin);
    authorize(e, id, admin, "grant_role", (&existing, &member, admin).into_val(e));
    let before = state(e, id);
    assert_eq!(
        access.try_grant_role(&existing, &member, admin),
        Err(Ok(Error::from_contract_error(2000)))
    );
    assert!(e.events().all().events().is_empty());
    assert_eq!(state(e, id), before);
    let noops: [(&str, Vec<Val>); 2] = [
        ("revoke_role", (&next, &member, &second_admin).into_val(e)),
        ("renounce_role", (&next, &second_admin).into_val(e)),
    ];
    for (name, args) in noops {
        e.mock_auths(&[]);
        let before = state(e, id);
        assert_eq!(
            e.try_invoke_contract::<(), Error>(id, &Symbol::new(e, name), args.clone()),
            Err(Ok(Error::from_type_and_code(ScErrorType::Context, ScErrorCode::InvalidAction)))
        );
        assert!(e.events().all().events().is_empty());
        assert_eq!(state(e, id), before);
        authorize(e, id, &second_admin, name, args.clone());
        e.invoke_contract::<()>(id, &Symbol::new(e, name), args);
        assert!(e.events().all().events().is_empty());
    }
    authorize(e, id, &second_admin, "renounce_role", (&admin_role, &second_admin).into_val(e));
    access.renounce_role(&admin_role, &second_admin);
    assert_eq!(access.get_role_member_count(&admin_role), 0);
    authorize(e, id, &second_admin, "grant_role", (&admin_role, admin, &second_admin).into_val(e));
    let before = state(e, id);
    assert_eq!(
        access.try_grant_role(&admin_role, admin, &second_admin),
        Err(Ok(Error::from_contract_error(2000)))
    );
    assert!(e.events().all().events().is_empty());
    assert_eq!(state(e, id), before);
}
