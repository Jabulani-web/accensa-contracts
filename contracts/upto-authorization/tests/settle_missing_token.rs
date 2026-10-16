//! Issue #694 — `settle` must report a missing token key as a typed error.
//!
//! `settle` used to `unwrap()` `DataKey::Token`, so a settle that reached the
//! transfer with no token configured aborted the host call with an untyped
//! `WasmRuntimeError` that callers and tests could not tell apart from a trap.
//! The tests below drop the key out of band (the only way to reach the arm,
//! since `initialize` writes `Admin` and `Token` together) and assert the
//! typed variant comes back, with a control proving the configured path still
//! settles.

use soroban_sdk::{
    testutils::Address as _,
    token::StellarAssetClient,
    Address, BytesN, Env,
};
use upto_authorization::{DataKey, Error, UptoAuthorization, UptoAuthorizationClient};

/// Buyer balance, large enough for every settle below.
const SUPPLY: i128 = 10_000_000;
/// Payment cap used by both tests.
const CAP: i128 = 1_000;

struct Setup {
    env: Env,
    client: UptoAuthorizationClient<'static>,
    contract: Address,
    payment_id: BytesN<32>,
}

/// A contract that is initialized with a token, holds a live authorization,
/// and has enough buyer balance to settle.
fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token = sac.address();
    StellarAssetClient::new(&env, &token).mint(&buyer, &SUPPLY);

    let contract = env.register(UptoAuthorization, ());
    let client = UptoAuthorizationClient::new(&env, &contract);
    client.initialize(&admin, &token);

    let payment_id = BytesN::from_array(&env, &[7u8; 32]);
    let expiry = env.ledger().sequence() + 10;
    client.authorize(&payment_id, &buyer, &seller, &CAP, &expiry);

    Setup {
        env,
        client,
        contract,
        payment_id,
    }
}

/// Control: with the token key present, settlement still transfers and marks
/// the authorization consumed, so the new error arm is the only difference
/// between the two tests.
#[test]
fn settle_succeeds_while_the_token_key_is_present() {
    let s = setup();

    assert_eq!(s.client.try_settle(&s.payment_id, &CAP), Ok(Ok(())));
}

/// The failing arm: `DataKey::Token` is absent when `settle` reaches the
/// transfer, so the call must return `Error::NotInitialized` — a value the
/// caller can match on — rather than abort the host call.
#[test]
fn settle_reports_a_missing_token_key_as_not_initialized() {
    let s = setup();

    // Drop the token key out of band: `initialize` writes `Admin` and `Token`
    // together, so this is the only way to reach line 397 with the key absent.
    s.env.as_contract(&s.contract, || {
        s.env.storage().instance().remove(&DataKey::Token);
    });

    // The admin and the authorization record are both still present, so every
    // guard before the transfer passes and the missing key is what is reported.
    assert_eq!(
        s.client.try_settle(&s.payment_id, &CAP),
        Err(Ok(Error::NotInitialized))
    );
}
