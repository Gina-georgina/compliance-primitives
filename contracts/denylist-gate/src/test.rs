use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{vec, Env, IntoVal, Map, Symbol, Val};

fn setup(env: &Env) -> (Address, Address, DenylistGateClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register(DenylistGate, ());
    let client = DenylistGateClient::new(env, &contract_id);
    client.initialize(&admin);
    (admin, contract_id, client)
}

#[test]
fn test_check_defaults_to_clear() {
    let env = Env::default();
    let (_admin, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    assert!(client.check(&alice));
}

#[test]
fn test_add_and_remove_from_denylist() {
    let env = Env::default();
    let (admin, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);

    client.add_to_denylist(&admin, &alice);
    assert!(!client.check(&alice));

    client.remove_from_denylist(&admin, &alice);
    assert!(client.check(&alice));
}

#[test]
fn test_add_to_denylist_rejects_non_admin() {
    let env = Env::default();
    let (_admin, _contract_id, client) = setup(&env);
    let impostor = Address::generate(&env);
    let alice = Address::generate(&env);

    let result = client.try_add_to_denylist(&impostor, &alice);
    assert_eq!(result, Err(Ok(Error::NotAuthorized)));
    assert!(client.check(&alice));
}

/// Soroban's `Address` type has no literal "empty" or "invalid" value the
/// way a raw string (`""`) would: every `Address` is either a well-formed
/// account or contract identifier, and the host rejects malformed ones
/// before they can ever reach contract code. So there is no empty-address
/// input to test directly.
///
/// What this test guards instead is the default-value invariant for a
/// storage key that has never been written: `check` reads
/// `DataKey::Denied(address)` and falls back via `unwrap_or(false)`, so an
/// untouched address must read as "clear" (`true`) rather than panicking
/// or defaulting to denied.
#[test]
fn test_empty_address_key_is_well_defined() {
    let env = Env::default();
    let (_admin, _contract_id, client) = setup(&env);
    let never_seen = Address::generate(&env);
    assert!(client.check(&never_seen));
}

#[test]
fn test_check_fresh_address_never_referenced_is_clear() {
    let env = Env::default();
    let (admin, _contract_id, client) = setup(&env);

    // Touch the denylist with other addresses so storage is not pristine.
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);
    client.add_to_denylist(&admin, &bob);
    client.add_to_denylist(&admin, &carol);
    client.remove_from_denylist(&admin, &carol);

    // A freshly generated address the contract has never seen in any call.
    let fresh = Address::generate(&env);
    assert!(client.check(&fresh));
    assert!(!client.check(&bob));
}

#[test]
fn test_add_then_remove_returns_to_default_clear_state() {
    let env = Env::default();
    let (admin, contract_id, client) = setup(&env);
    let alice = Address::generate(&env);

    client.add_to_denylist(&admin, &alice);
    assert!(!client.check(&alice));

    client.remove_from_denylist(&admin, &alice);
    assert!(client.check(&alice));

    // The storage entry itself must be gone, not left behind as a stale
    // `false` or `true` value.
    env.as_contract(&contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKey::Denied(alice.clone())));
    });
}

#[test]
fn test_remove_from_denylist_never_added_is_noop() {
    let env = Env::default();
    let (admin, contract_id, client) = setup(&env);
    let never_added = Address::generate(&env);

    assert!(client.check(&never_added));

    client.remove_from_denylist(&admin, &never_added);

    assert_eq!(
        env.events().all(),
        vec![
            &env,
            (
                contract_id.clone(),
                (Symbol::new(&env, "deny_remove"), never_added.clone()).into_val(&env),
                Map::<Symbol, Val>::new(&env).into_val(&env),
            ),
        ]
    );
    assert!(client.check(&never_added));
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    let (admin, _contract_id, client) = setup(&env);
    let result = client.try_initialize(&admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}
