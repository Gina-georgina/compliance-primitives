//! `jurisdiction-flag` is a `#![no_std]` Soroban contract that attaches a
//! jurisdiction code (e.g. an ISO 3166-1 alpha-2 country code) to an
//! address.
//!
//! **Purpose**: let an issuer record which jurisdiction an address has been
//! verified in, so other contracts can restrict activity to a permitted set
//! of jurisdictions without each one reimplementing that bookkeeping.
//!
//! **Callers**: only the configured `issuer` address may call
//! `set_jurisdiction`. Any contract or off-chain client can read a flag via
//! `get_jurisdiction`, and contracts enforcing a jurisdiction allowlist can
//! call `is_permitted_jurisdiction(address, allowed_codes)` directly as part
//! of their own compliance checks.
//!
//! **Composition**: designed to be called into from another contract's
//! `transfer` or similar gating logic — the same pattern `denylist-gate`
//! uses — rather than deployed standalone.
#![no_std]

use soroban_sdk::{contract, contracterror, contractevent, contractimpl, contracttype, Address, Env, String, Vec};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Issuer,
    Jurisdiction(Address),
}

#[contractevent]
pub struct JurisdictionSet {
    #[topic]
    pub address: Address,
    pub code: String,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    InvalidJurisdictionCode = 4,
}

#[contract]
pub struct JurisdictionFlag;

#[contractimpl]
impl JurisdictionFlag {
    /// One-time setup that records `issuer` as the only address allowed to
    /// set jurisdiction codes afterward.
    ///
    /// # Parameters
    /// - `issuer`: the address that will be authorized to call
    ///   [`set_jurisdiction`](Self::set_jurisdiction).
    ///
    /// # Auth
    /// Requires `issuer.require_auth()`, so the issuer must sign the
    /// initialization.
    ///
    /// # Errors
    /// - [`Error::AlreadyInitialized`] if the contract has already been
    ///   initialized. The existing issuer is left unchanged.
    pub fn initialize(env: Env, issuer: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Issuer) {
            return Err(Error::AlreadyInitialized);
        }
        issuer.require_auth();
        env.storage().instance().set(&DataKey::Issuer, &issuer);
        Ok(())
    }

    /// Attaches jurisdiction `code` to `address`, overwriting any code
    /// previously set for it, and emits a [`JurisdictionSet`] event.
    ///
    /// # Parameters
    /// - `issuer`: the caller; must match the issuer recorded by
    ///   [`initialize`](Self::initialize).
    /// - `address`: the address whose jurisdiction is being recorded.
    /// - `code`: the jurisdiction code (e.g. an ISO 3166-1 alpha-2 country
    ///   code such as `"US"`). Must be non-empty.
    ///
    /// # Auth
    /// Issuer-only. Requires `issuer.require_auth()`.
    ///
    /// # Errors
    /// - [`Error::NotInitialized`] if [`initialize`](Self::initialize) has
    ///   not been called yet.
    /// - [`Error::NotAuthorized`] if `issuer` is not the stored issuer.
    /// - [`Error::InvalidJurisdictionCode`] if `code` is an empty string.
    ///
    /// On any error nothing is written to storage and no event is emitted.
    pub fn set_jurisdiction(
        env: Env,
        issuer: Address,
        address: Address,
        code: String,
    ) -> Result<(), Error> {
        Self::require_issuer(&env, &issuer)?;
        if code.len() == 0 {
            return Err(Error::InvalidJurisdictionCode);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Jurisdiction(address.clone()), &code);
        JurisdictionSet { address, code }.publish(&env);
        Ok(())
    }

    /// Returns the jurisdiction code attached to `address`, if any.
    ///
    /// # Parameters
    /// - `address`: the address to look up.
    ///
    /// # Returns
    /// `Some(code)` if a code has been set via
    /// [`set_jurisdiction`](Self::set_jurisdiction), otherwise `None`.
    ///
    /// # Auth
    /// None. This is a read-only call anyone may make.
    ///
    /// # Errors
    /// Never fails. Works even before the contract is initialized, in which
    /// case it always returns `None`.
    pub fn get_jurisdiction(env: Env, address: Address) -> Option<String> {
        env.storage().persistent().get(&DataKey::Jurisdiction(address))
    }

    /// Checks whether `address` is in one of the `allowed_codes`
    /// jurisdictions. Meant to be called by other contracts that want to
    /// restrict activity to a set of permitted jurisdictions.
    ///
    /// # Parameters
    /// - `address`: the address to check.
    /// - `allowed_codes`: the jurisdiction codes the caller permits.
    ///
    /// # Returns
    /// `true` only if `address` has a jurisdiction code set AND that code
    /// appears in `allowed_codes` (exact, case-sensitive match). Returns
    /// `false` if no code is set or if `allowed_codes` is empty.
    ///
    /// # Auth
    /// None. This is a read-only call anyone may make.
    ///
    /// # Errors
    /// Never fails. Before initialization no codes can have been set, so
    /// it always returns `false`.
    pub fn is_permitted_jurisdiction(env: Env, address: Address, allowed_codes: Vec<String>) -> bool {
        match Self::get_jurisdiction(env, address) {
            Some(code) => allowed_codes.iter().any(|c| c == code),
            None => false,
        }
    }

    fn require_issuer(env: &Env, issuer: &Address) -> Result<(), Error> {
        issuer.require_auth();
        let stored_issuer: Address = env
            .storage()
            .instance()
            .get(&DataKey::Issuer)
            .ok_or(Error::NotInitialized)?;
        if stored_issuer != *issuer {
            return Err(Error::NotAuthorized);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test;
