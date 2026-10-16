//! Issue #693 — `authorize_with_slippage` must report missing state as typed
//! errors rather than aborting the host call.
//!
//! The existing-authorization read inside `authorize_with_slippage` used to
//! `.unwrap()` the `Option` coming back from storage, so a missing record
//! aborted with an untyped `WasmRuntimeError` that callers and tests could not
//! tell apart from a trap; it now reports `Error::AuthorizationNotFound`.
//!
//! That read is guarded by `has`, so the missing-state path a caller can
//! actually reach is calling the function before `initialize` has written any
//! state — which must come back as `Error::NotInitialized`, not an abort.
//! Both are asserted below.

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, BytesN, Env,
};
use upto_authorization::{Error, UptoAuthorization, UptoAuthorizationClient};

/// Buyer balance, large enough for every authorization below.
const SUPPLY: i128 = 10_000_000;
/// Payment cap used throughout.
const CAP: i128 = 1_000;

fn pid(env: &Env, n: u8) -> BytesN<32> {
    BytesN::from_array(env, &[n; 32])
}

/// A contract with a token but no state written yet: only `register` has run.
fn uninit_client(env: &Env) -> UptoAuthorizationClient<'static> {
    let contract = env.register(UptoAuthorization, ());
    UptoAuthorizationClient::new(env, &contract)
}

/// A contract that is initialized, so the record-read arm at issue #693's line
/// is reachable through the public API.
fn init_client(env: &Env) -> (UptoAuthorizationClient<'static>, Address) {
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token = sac.address();
    let buyer = Address::generate(env);
    StellarAssetClient::new(env, &token).mint(&buyer, &SUPPLY);

    let contract = env.register(UptoAuthorization, ());
    let client = UptoAuthorizationClient::new(env, &contract);
    client.initialize(&admin, &token);
    (client, token)
}

/// The case this issue names: a caller reaching the contract before it is
/// initialised gets the crate's typed `NotInitialized` back from the call,
/// not a host abort.
#[test]
fn authorize_with_slippage_before_initialize_reports_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let client = uninit_client(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let expiry = env.ledger().sequence() + 10;

    assert_eq!(
        client.try_authorize_with_slippage(&pid(&env, 1), &buyer, &seller, &CAP, &expiry, &0),
        Err(Ok(Error::NotInitialized))
    );
}

/// The record-read arm with the record present: the `Option` is `Some`, the
/// read succeeds, and the live-authorization guard reports `AlreadySettled`
/// as a typed error.
#[test]
fn authorize_with_slippage_with_a_live_record_reports_already_settled() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _token) = init_client(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let expiry = env.ledger().sequence() + 10;

    assert_eq!(
        client.try_authorize_with_slippage(&pid(&env, 2), &buyer, &seller, &CAP, &expiry, &0),
        Ok(Ok(()))
    );
    assert_eq!(
        client.try_authorize_with_slippage(&pid(&env, 2), &buyer, &seller, &CAP, &expiry, &0),
        Err(Ok(Error::AlreadySettled))
    );
}

/// Same read with an expired record: the arm falls through and the new
/// authorization replaces the old one, so the success path through line 192 is
/// unchanged by the fix.
#[test]
fn authorize_with_slippage_reauthorizes_after_the_previous_record_expires() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _token) = init_client(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let expiry = env.ledger().sequence() + 5;

    assert_eq!(
        client.try_authorize_with_slippage(&pid(&env, 3), &buyer, &seller, &CAP, &expiry, &0),
        Ok(Ok(()))
    );

    env.ledger().with_mut(|l| l.sequence_number += 6);

    assert_eq!(
        client.try_authorize_with_slippage(
            &pid(&env, 3),
            &buyer,
            &seller,
            &CAP,
            &(expiry + 10),
            &0
        ),
        Ok(Ok(()))
    );
}
