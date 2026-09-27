extern crate std;

use super::*;
use soroban_sdk::testutils::storage::Persistent as _;
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
fn test_add_and_remove_without_audit_log_is_unchanged() {
    let env = Env::default();
    let (admin, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);

    assert!(client.check(&alice));

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

#[test]
fn test_remove_from_denylist_never_added_is_noop() {
    let env = Env::default();
    let (admin, _contract_id, client) = setup(&env);
    let never_added = Address::generate(&env);

    assert!(client.check(&never_added));
    client.remove_from_denylist(&admin, &never_added);
    assert!(client.check(&never_added));
}

#[test]
fn test_remove_multiple_from_denylist_removes_all_without_audit_log_noise() {
    let env = Env::default();
    let (admin, _contract_id, client) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.add_to_denylist(&admin, &alice);
    client.add_to_denylist(&admin, &bob);

    client.remove_multiple_from_denylist(&admin, &vec![&env, alice.clone(), bob.clone()]);

    assert!(client.check(&alice));
    assert!(client.check(&bob));
}

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    let (admin, _contract_id, client) = setup(&env);
    let result = client.try_initialize(&admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}
