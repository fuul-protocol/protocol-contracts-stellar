use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env, U256};

use crate::types::*;

#[test]
fn claim_authorization_binds_every_business_field_and_excludes_signer_selection() {
    let e = Env::default();
    let project = Address::generate(&e);
    let to = Address::generate(&e);
    let currency = Address::generate(&e);
    let signer = Address::generate(&e);
    let proof = BytesN::from_array(&e, &[7; 32]);
    let claim = ClaimCheck {
        project_address: project.clone(),
        to: to.clone(),
        currency: currency.clone(),
        currency_type: TokenType::StellarAsset,
        amount: 42,
        reason: ClaimReason::AffiliatePayout,
        token_id: U256::from_u32(&e, 9),
        deadline: U256::from_u32(&e, 123),
        proof: proof.clone(),
        signers: vec![&e, signer],
    };

    assert_eq!(
        claim.authorization(),
        ClaimAuthorization {
            project_address: project,
            to,
            currency,
            amount: 42,
            reason: ClaimReason::AffiliatePayout,
            token_id: U256::from_u32(&e, 9),
            deadline: U256::from_u32(&e, 123),
            proof,
        }
    );
}
