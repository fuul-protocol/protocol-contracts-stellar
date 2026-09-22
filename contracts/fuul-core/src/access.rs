use soroban_sdk::{contracttrait, Address, Env, Symbol, Vec};
use stellar_access::access_control as oz;

pub fn default_admin_role(e: &Env) -> Symbol {
    Symbol::new(e, "default_admin")
}

pub fn require_admin(e: &Env, caller: &Address) {
    caller.require_auth();
    oz::ensure_role(e, &default_admin_role(e), caller);
}

fn revoke_role_no_auth(e: &Env, role: &Symbol, account: &Address, caller: &Address) {
    if oz::has_role(e, account, role).is_some() {
        oz::revoke_role_no_auth(e, account, role, caller);
    }
}

/// Shared multi-administrator policy backed by OpenZeppelin role storage and events.
/// Queries are role-first; mutators authenticate the explicit caller.
/// Revoking or renouncing an absent role is an authenticated no-op.
#[contracttrait]
pub trait FuulAccessControl {
    fn default_admin_role(e: &Env) -> Symbol {
        default_admin_role(e)
    }

    fn has_role(e: &Env, role: Symbol, account: Address) -> bool {
        oz::has_role(e, &account, &role).is_some()
    }

    fn get_role_admin(e: &Env, role: Symbol) -> Symbol {
        let _ = role;
        default_admin_role(e)
    }

    fn get_role_member(e: &Env, role: Symbol, index: u32) -> Address {
        oz::get_role_member(e, &role, index)
    }

    fn get_role_member_count(e: &Env, role: Symbol) -> u32 {
        oz::get_role_member_count(e, &role)
    }

    fn get_role_members(e: &Env, role: Symbol) -> Vec<Address> {
        let mut members = Vec::new(e);
        for index in 0..oz::get_role_member_count(e, &role) {
            members.push_back(oz::get_role_member(e, &role, index));
        }
        members
    }

    fn grant_role(e: &Env, role: Symbol, account: Address, caller: Address) {
        require_admin(e, &caller);
        oz::grant_role_no_auth(e, &account, &role, &caller);
    }

    fn revoke_role(e: &Env, role: Symbol, account: Address, caller: Address) {
        require_admin(e, &caller);
        revoke_role_no_auth(e, &role, &account, &caller);
    }

    fn renounce_role(e: &Env, role: Symbol, caller: Address) {
        caller.require_auth();
        revoke_role_no_auth(e, &role, &caller, &caller);
    }
}
