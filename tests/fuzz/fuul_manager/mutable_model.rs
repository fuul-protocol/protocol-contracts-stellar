use crate::test::{
    roles_pause_helpers::test_env,
    security_helpers::{claim_payload, state},
    *,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn mutable_cooldown_two_currency_batches_match_independent_model_with_revoked_shared_roles(
        quorum in 1_u32..3,
        operations in prop::collection::vec((any::<bool>(),0_u8..101,0_u8..101,any::<bool>(),0_u32..172_801,any::<bool>()),1..16)
    ) {
        let env=test_env();
        let f=claim_fixture(&env);
        let other=Address::generate(&env);
        f.client.add_currency_limit(&f.admin,&other,&u(&env,100));
        f.client.set_currency_token_limit(&f.admin,&f.currency,&u(&env,100));
        let currencies=[f.currency.clone(),other];
        let extra=Address::generate(&env);
        let backup=Address::generate(&env);
        let access=FuulAccessControlClient::new(&env,&f.client.address);
        let role=Symbol::new(&env,"claim_signer");
        access.grant_role(&role,&backup,&f.admin);
        if quorum==2 { f.client.set_required_signers(&f.admin,&2); }
        let project=register_mock_project(&env,&f.admin,0);
        let mut cooldown=86_400_u64;
        let mut starts=[1_000_000_u64;2];
        let mut accumulated=[0_i128;2];
        let mut totals=[0_i128;2];
        for (index,(reverse,a,b,long,advance,revoke)) in operations.into_iter().enumerate() {
            env.mock_all_auths();
            access.grant_role(&role,&extra,&f.admin);
            let updated=if long {172_800} else {86_400};
            if cooldown!=updated { f.client.set_claim_cooldown(&f.admin,&u128::from(updated));cooldown=updated; }
            env.ledger().with_mut(|l|l.timestamp+=u64::from(advance));
            let now=env.ledger().timestamp();
            let order=if reverse {[1,0]} else {[0,1]};
            let amounts=[i128::from(a),i128::from(b)];
            let mut first=claim_check(&env,&f,&project.address,amounts[0],(index*2) as u8);
            first.currency=currencies[order[0]].clone();
            if quorum==2 { first.signers.push_back(backup.clone()); }
            let mut second=claim_check(&env,&f,&project.address,amounts[1],(index*2+1) as u8);
            second.currency=currencies[order[1]].clone();
            second.signers.push_back(extra.clone());
            let checks=vec![&env,first.clone(),second.clone()];
            // Approval payloads are prepared while all participating signers still have their roles.
            let first_payload=claim_payload(&env,&first);
            let second_payload=claim_payload(&env,&second);
            if revoke { access.revoke_role(&role,&extra,&f.admin); }
            let mut next_starts=starts;
            let mut next_accumulated=accumulated;
            let mut expected=None;
            for step in 0..2 {
                let currency=order[step];
                if now>=starts[currency]+cooldown { next_starts[currency]=now;next_accumulated[currency]=0; }
                if next_accumulated[currency]+amounts[step]>100 { expected=Some(6304);break; }
                next_accumulated[currency]+=amounts[step];
                if step==1 && revoke { expected=Some(6307);break; }
            }
            let before=state(&env,&f.client.address);
            env.mock_auths(&[
                MockAuth { address:&f.caller,invoke:&MockAuthInvoke { contract:&f.client.address,fn_name:"claim",args:(&f.caller,&checks).into_val(&env),sub_invokes:&[] } },
                MockAuth { address:&f.signer,invoke:&MockAuthInvoke { contract:&f.client.address,fn_name:"claim",args:first_payload.clone(),sub_invokes:&[] } },
                MockAuth { address:&f.signer,invoke:&MockAuthInvoke { contract:&f.client.address,fn_name:"claim",args:second_payload.clone(),sub_invokes:&[] } },
                MockAuth { address:&extra,invoke:&MockAuthInvoke { contract:&f.client.address,fn_name:"claim",args:second_payload,sub_invokes:&[] } },
                MockAuth { address:&backup,invoke:&MockAuthInvoke { contract:&f.client.address,fn_name:"claim",args:first_payload,sub_invokes:&[] } },
            ]);
            let actual=f.client.try_claim(&f.caller,&checks);
            if let Some(code)=expected {
                prop_assert_eq!(actual,Err(Ok(Error::from_contract_error(code))));
                prop_assert!(env.events().all().events().is_empty());
                prop_assert_eq!(state(&env,&f.client.address),before);
            } else {
                prop_assert_eq!(actual,Ok(Ok(())));
                starts=next_starts;accumulated=next_accumulated;
                for step in 0..2 { totals[order[step]]+=amounts[step]; }
            }
            for currency in 0..2 {
                prop_assert_eq!(f.client.currency_limits(&currencies[currency]),CurrencyTokenLimit { claim_limit_per_cooldown:u(&env,100),cumulative_claim_per_cooldown:u(&env,accumulated[currency]),claim_cooldown_period_started:starts[currency] });
                prop_assert_eq!(f.client.users_claims(&f.recipient,&currencies[currency]),u(&env,totals[currency]));
            }
        }
    }
}
