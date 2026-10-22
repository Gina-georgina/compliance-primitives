//! ## Storage and TTL policy
//!
//! All state (admin, pending admin and the frozen flag) lives in **instance**
//! storage, which shares a single TTL with the contract instance itself. Every
//! write path (`initialize`, `freeze`, `unfreeze`, `propose_admin`,
//! `accept_admin`) finishes by extending that TTL: whenever the remaining TTL
//! has dropped below [`INSTANCE_TTL_THRESHOLD`] (~7 days) it is bumped back up
//! to [`INSTANCE_TTL_EXTEND_TO`] (~90 days). This keeps the breaker from being
//! archived by Soroban state expiration — an archived breaker would make
//! `is_frozen` read as its default and leave the freeze unenforceable until a
//! manual restore.
#![no_std]

//! ## Upgradeability
//!
//! `upgrade(admin, new_wasm_hash)` lets the configured admin move the
//! contract's code to a new WASM hash via
//! `env.deployer().update_current_contract_wasm`, following the same
//! admin-gated pattern as `compliance-aggregator::upgrade`. Storage (the admin
//! key, the frozen flag and any pending admin) is untouched by the WASM swap,
//! so the freeze state survives the upgrade and the contract remains callable
//! immediately after it. Only the admin can call `upgrade`; any other caller
//! is rejected with `Error::NotAuthorized`.
//!
//! If a new version changes the storage layout, ship a migration entrypoint
//! alongside it and call it after `upgrade`. See `STORAGE_VERSIONING.md`.

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype, Address,
    BytesN, Env, String, Symbol,
};

/// Extend the contract's instance storage when its remaining TTL drops below
/// this many ledgers (~7 days at ~5s/ledger).
pub const INSTANCE_TTL_THRESHOLD: u32 = 120_960;

/// Target remaining TTL, in ledgers, after an extension (~90 days at
/// ~5s/ledger).
pub const INSTANCE_TTL_EXTEND_TO: u32 = 1_555_200;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Frozen,
    PendingAdmin,
    AuditLog,
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

#[contractclient(name = "AuditLogClient")]
pub trait AuditLogInterface {
    fn record(env: Env, source: Address, kind: Symbol, subject: Address, detail: String);
}

/// Emitted whenever the contract is upgraded to a new WASM implementation,
/// for on-chain auditability of the upgrade path.
#[contractevent]
pub struct UpgradePerformed {
    #[topic]
    pub admin: Address,
    pub new_wasm_hash: BytesN<32>,
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
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    /// Configure the optional append-only audit log. Admin-only.
    pub fn set_audit_log(env: Env, admin: Address, audit_log: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::AuditLog, &audit_log);
        Ok(())
    }

    pub fn freeze(env: Env, admin: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        // Idempotency: if already frozen, skip the write and event so that
        // indexers and dashboards see exactly one Frozen event per incident,
        // and freeze-duration metrics remain correct.
        let already_frozen: bool = env
            .storage()
            .instance()
            .get(&DataKey::Frozen)
            .unwrap_or(false);
        if already_frozen {
            return Ok(());
        }
        env.storage().instance().set(&DataKey::Frozen, &true);
        Self::extend_instance_ttl(&env);
        Frozen {
            admin: admin.clone(),
        }
        .publish(&env);
        Self::record_audit(&env, Symbol::new(&env, "freeze"), &admin, "circuit breaker frozen");
        Ok(())
    }

    pub fn unfreeze(env: Env, admin: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        // Idempotency: if already unfrozen, skip the write and event so that
        // duplicate calls from retrying scripts don't produce spurious events.
        let currently_frozen: bool = env
            .storage()
            .instance()
            .get(&DataKey::Frozen)
            .unwrap_or(false);
        if !currently_frozen {
            return Ok(());
        }
        env.storage().instance().set(&DataKey::Frozen, &false);
        Self::extend_instance_ttl(&env);
        Unfrozen {
            admin: admin.clone(),
        }
        .publish(&env);
        Self::record_audit(&env, Symbol::new(&env, "unfreeze"), &admin, "circuit breaker unfrozen");
        Ok(())
    }

    /// Returns `Ok(true)` if the breaker is frozen, `Ok(false)` if it is
    /// unfrozen.
    ///
    /// Returns `Err(Error::NotInitialized)` if the contract has never been
    /// initialized — i.e. `initialize` was never called.  Callers **must**
    /// treat this error as a hard failure (deny the transfer / flag a
    /// misconfiguration) rather than assuming the breaker is open.  This
    /// prevents the silent fail-open behaviour that occurred when
    /// `unwrap_or(false)` was used: a consumer pointing at a freshly-deployed
    /// but uninitialized contract would see `is_frozen == false` and happily
    /// allow all transfers even though no legitimate admin had ever set the
    /// breaker up.
    pub fn is_frozen(env: Env) -> Result<bool, Error> {
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::NotInitialized);
        }
        Ok(env
            .storage()
            .instance()
            .get(&DataKey::Frozen)
            .unwrap_or(false))
    }

    /// Returns the current admin address, or `Err(Error::NotInitialized)` if
    /// the contract has not yet been initialized.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
    }

    /// Propose a new admin. The current admin remains fully in control
    /// until the proposed admin calls `accept_admin`; this avoids a
    /// single-typo lockout from a direct admin overwrite.
    pub fn propose_admin(env: Env, admin: Address, new_admin: Address) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::PendingAdmin, &new_admin);
        Self::extend_instance_ttl(&env);
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
        Self::extend_instance_ttl(&env);
        Ok(())
    }

    /// Upgrade the contract WASM. Admin-only.
    ///
    /// Swaps the code behind this contract ID to `new_wasm_hash` (which must
    /// already be uploaded to the network). The admin, frozen flag and pending
    /// admin are preserved across the upgrade. See the module-level
    /// "Upgradeability" section for the full procedure.
    pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        Self::require_admin(&env, &admin)?;
        UpgradePerformed {
            admin,
            new_wasm_hash: new_wasm_hash.clone(),
        }
        .publish(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
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

    /// Refresh the TTL of the contract instance (and therefore of every
    /// instance-storage entry). See the "Storage and TTL policy" section of
    /// the module docs.
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
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

    fn record_audit(env: &Env, kind: Symbol, subject: &Address, detail: &str) {
        if let Some(audit_log) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::AuditLog)
        {
            AuditLogClient::new(env, &audit_log).record(
                &env.current_contract_address(),
                &kind,
                subject,
                &String::from_str(env, detail),
            );
        }
    }
}

#[cfg(test)]
mod test;
