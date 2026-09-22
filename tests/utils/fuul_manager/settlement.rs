use crate::test::*;
use soroban_sdk::{xdr, Bytes, TryFromVal};
use std::rc::Rc;

pub(super) struct Settlement<'a> {
    pub manager: FuulManagerClient<'a>,
    pub factory: FuulFactoryClient<'a>,
    pub admin: Address,
    pub signers: Vec<Address>,
    pub projects: [Address; 2],
    pub currency: Address,
    pub native: Address,
    pub funder: Address,
    pub payer: Address,
    pub recipient: Address,
    pub collector: Address,
}

pub(super) fn setup(e: &Env, compiled: bool) -> Settlement<'_> {
    e.ledger().with_mut(|l| l.timestamp = 1_000_000);
    let admin = Address::generate(e);
    let currency = e.register_stellar_asset_contract_v2(admin.clone()).address();
    // A funded ledger account backs the real Asset::Native SAC; native XLM cannot be minted.
    let account_id = xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256([88; 32])));
    e.host()
        .add_ledger_entry(
            &Rc::new(xdr::LedgerKey::Account(xdr::LedgerKeyAccount {
                account_id: account_id.clone(),
            })),
            &Rc::new(xdr::LedgerEntry {
                last_modified_ledger_seq: 0,
                ext: xdr::LedgerEntryExt::V0,
                data: xdr::LedgerEntryData::Account(xdr::AccountEntry {
                    account_id: account_id.clone(),
                    balance: 1_000_000_000_000_000,
                    seq_num: xdr::SequenceNumber(0),
                    num_sub_entries: 0,
                    inflation_dest: None,
                    flags: 0,
                    home_domain: Default::default(),
                    thresholds: xdr::Thresholds([1; 4]),
                    signers: Default::default(),
                    ext: xdr::AccountEntryExt::V0,
                }),
            }),
            None,
        )
        .unwrap();
    let funder = Address::try_from_val(e, &xdr::ScAddress::Account(account_id)).unwrap();
    let native = e.deployer().with_stellar_asset(Bytes::from_array(e, &[0; 4])).deploy();
    assert_eq!(TokenClient::new(e, &currency).decimals(), 7);
    assert_eq!(TokenClient::new(e, &native).decimals(), 7);
    let signers = vec![e, Address::generate(e), Address::generate(e), Address::generate(e)];
    let args = (
        &admin,
        &admin,
        &admin,
        1_u128,
        &signers,
        &currency,
        &native,
        None::<Address>,
        u(e, 1_000_000_000_000_i128),
    );
    let manager_id = if compiled {
        e.register(constructor_helpers::MANAGER_WASM, args)
    } else {
        e.register(FuulManager, args)
    };
    let code = e.deployer().upload_contract_wasm(PROJECT_WASM);
    let collector = Address::generate(e);
    let factory_id = e.register(FuulFactory, (&admin, &manager_id, &collector, &code));
    let factory = FuulFactoryClient::new(e, &factory_id);
    e.mock_all_auths();
    let projects = ["ipfs://source-a", "ipfs://source-b"]
        .map(|uri| factory.create_fuul_project(&admin, &String::from_str(e, uri), &false));
    let payer = Address::generate(e);
    TokenClient::new(e, &native).transfer(&funder, MuxedAddress::from(&payer), &10_000_000_000);
    Settlement {
        manager: FuulManagerClient::new(e, &manager_id),
        factory,
        admin,
        signers,
        projects,
        currency,
        native,
        funder,
        payer,
        recipient: Address::generate(e),
        collector,
    }
}

pub(super) fn fees(f: &Settlement<'_>, project: usize, bps: u32, native: i128) {
    let p = &f.projects[project];
    let old = f.factory.project_fees(p);
    if old.project_claim_fee != bps {
        f.factory.set_project_claim_fee(&f.admin, p, &bps);
    }
    if old.native_user_claim_fee != native {
        f.factory.set_native_user_claim_fee(&f.admin, p, &native);
    }
}

pub(super) fn fund(e: &Env, f: &Settlement<'_>, asset: &Address, project: &Address, amount: i128) {
    if asset == &f.native {
        TokenClient::new(e, asset).transfer(&f.funder, MuxedAddress::from(project), &amount);
    } else {
        StellarAssetClient::new(e, asset).mint(project, &amount);
    }
}

pub(super) fn check(
    e: &Env,
    f: &Settlement<'_>,
    p: usize,
    asset: &Address,
    amount: i128,
    proof: u8,
) -> ClaimCheck {
    ClaimCheck {
        project_address: f.projects[p].clone(),
        to: f.recipient.clone(),
        currency: asset.clone(),
        currency_type: TokenType::StellarAsset,
        amount,
        reason: ClaimReason::AffiliatePayout,
        token_id: u(e, 0),
        deadline: u(e, e.ledger().timestamp() + 3_600),
        proof: BytesN::from_array(e, &[proof; 32]),
        signers: vec![e, f.signers.get(0).unwrap()],
    }
}

pub(super) fn assert_claim_events(e: &Env, f: &Settlement<'_>, checks: &Vec<ClaimCheck>) {
    let expected: std::vec::Vec<_> = checks
        .iter()
        .map(|c| {
            Claimed {
                project_address: c.project_address,
                to: c.to,
                currency: c.currency,
                amount: c.amount,
                currency_type: c.currency_type,
                token_id: c.token_id,
                reason: c.reason,
                proof: c.proof,
            }
            .to_xdr(e, &f.manager.address)
        })
        .collect();
    assert_eq!(e.events().all().filter_by_contract(&f.manager.address), expected);
}

pub(super) fn fee(e: &Env, amount: i128, bps: u32) -> i128 {
    U256::from_u128(e, amount.try_into().unwrap())
        .mul(&U256::from_u32(e, bps))
        .div(&U256::from_u32(e, 10_000))
        .to_u128()
        .unwrap()
        .try_into()
        .unwrap()
}
