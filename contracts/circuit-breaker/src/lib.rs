//! `circuit-breaker` is a `#![no_std]` Soroban contract that holds a single,
//! deployment-wide **emergency freeze flag**. Consumer contracts read the
//! flag with `is_frozen()` and refuse to proceed while it is set.
//!
//! ## Purpose
//!
//! During an incident (an exploited bug, a sanctions designation that needs
//! immediate effect, a compromised issuer key) an operator needs to stop
//! compliance-gated activity everywhere at once. Pausing each primitive
//! separately is slow and error-prone, so this contract provides one switch:
//! `freeze` once, and every consumer that checks the flag fails closed.
//! Unlike the per-contract `pausable` mechanism, the freeze is global to the
//! deployment, not scoped to one contract.
//!
//! ## Who calls it
//!
//! - **The admin** calls `initialize` once, then `freeze` and `unfreeze`.
//!   Every mutation requires the stored admin's auth. In production the admin
//!   should be a `multisig-admin` contract address rather than a single key
//!   (see `docs/emergency-freeze-design.md`).
//! - **Admin transfer** is two-step: the current admin calls `propose_admin`,
//!   and the proposed address calls `accept_admin`. The current admin keeps
//!   control until the transfer is accepted, which avoids a mistyped address
//!   locking the contract.
//! - **Consumer contracts** (token and transfer contracts, policy evaluators)
//!   call `is_frozen()` before running their own checks. The call is
//!   permissionless and requires no auth.
//! - **Off-chain tooling and monitors** read `is_frozen()` and `metadata()`,
//!   and follow the `Frozen` / `Unfrozen` events to alert on state changes.
//!
//! ## How it composes with the other contracts
//!
//! | Contract | Relationship to `circuit-breaker` |
//! |---|---|
//! | `policy-engine` | Optional. Configured with `set_circuit_breaker`; when the flag is set, `evaluate` denies without running any check. |
//! | `denylist-gate`, `allowlist-token`, `jurisdiction-flag` | Not wired to it directly. A token or wrapper that uses these primitives should call `is_frozen()` first, so the freeze stops transfers before any per-primitive check runs. |
//! | `multisig-admin` | Recommended value for the `admin` address, so freezing and unfreezing require M-of-N approval. |
//! | `pausable` | Complementary. Local, per-contract pause for routine maintenance; this contract is the incident-wide stop. |
//! | `audit-log` | Complementary. Integrators that need a contract-readable trail of freeze events record them there; this contract only emits events. |
//!
//! ## Limits
//!
//! `is_frozen()` is advisory. This contract cannot force a consumer to check
//! it, so a consumer that omits the check gets no protection. Freezing also
//! does not modify any state in the other contracts: existing balances,
//! allowlist entries, and denylist entries are left as they are.
//!
//! See `SPEC.md` (section 5) for the full security assumptions and
//! `EVENTS.md` for the event schema.

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env, String,
};

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Frozen,
    PendingAdmin,
}

/// Version + admin metadata about this contract instance, for tooling that
/// introspects deployed contracts.
#[contracttype]
#[derive(Clone)]
pub struct Metadata {
    pub version: String,
    pub admin: Address,
}

#[contractevent]
pub struct Frozen {
    #[topic]
    pub admin: Address,
}

#[contractevent]
pub struct Unfrozen {
    #[topic]
    pub admin: Address,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    NotAuthorized = 3,
    NoPendingAdmin = 4,
    PendingAdminMismatch = 5,
}

#[contract]
pub struct CircuitBreaker;

#[contractimpl]
impl CircuitBreaker {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Frozen, &false);
        Ok(())
    }

    pub fn freeze(env: Env, admin: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Frozen, &true);
        Frozen {
            admin: admin.clone(),
        }
        .publish(&env);
        Ok(())
    }

    pub fn unfreeze(env: Env, admin: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Frozen, &false);
        Unfrozen {
            admin: admin.clone(),
        }
        .publish(&env);
        Ok(())
    }

    pub fn is_frozen(env: Env) -> bool {
        env.storage().instance().get(&DataKey::Frozen).unwrap_or(false)
    }

    /// Propose a new admin. The current admin remains fully in control
    /// until the proposed admin calls `accept_admin`; this avoids a
    /// single-typo lockout from a direct admin overwrite.
    pub fn propose_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::PendingAdmin, &new_admin);
        Ok(())
    }

    /// Accept a pending admin transfer. Must be called by the proposed
    /// admin itself.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        new_admin.require_auth();
        let pending_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(Error::NoPendingAdmin)?;
        if pending_admin != new_admin {
            return Err(Error::PendingAdminMismatch);
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        Ok(())
    }

    /// Returns metadata about this contract instance, including version and
    /// admin address.
    pub fn metadata(env: Env) -> Result<Metadata, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        Ok(Metadata {
            version: String::from_str(&env, env!("CARGO_PKG_VERSION")),
            admin,
        })
    }

    fn require_admin(env: &Env, admin: &Address) -> Result<(), Error> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if stored_admin != *admin {
            return Err(Error::NotAuthorized);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test;
