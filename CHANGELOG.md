# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `jurisdiction-flag`: `Error::InvalidJurisdictionCode` variant.

### Changed

- `jurisdiction-flag`: `set_jurisdiction` now rejects an empty-string `code`
  with `Error::InvalidJurisdictionCode` instead of storing it.

## [0.1.0] - 2026-07-14

### Added

- `allowlist-token`: a Soroban contract that wraps an existing SEP-41 token
  and only forwards `transfer` calls when both `from` and `to` are on an
  admin-managed allowlist. Exposes `initialize`, `add_to_allowlist`,
  `remove_from_allowlist`, `is_allowed` and `transfer`. Blocked transfers
  return `Ok(false)` and emit a `Blocked` event; allowlist changes emit
  `AllowAdd` / `AllowRemove` events.
- `denylist-gate`: a standalone on-chain denylist meant to be called into by
  other contracts. Exposes `initialize`, `add_to_denylist`,
  `remove_from_denylist` and the read-only `check(address)`, which returns
  `true` when an address is clear to transact. Denylist changes emit
  `DenyAdd` / `DenyRemove` events.
- `jurisdiction-flag`: a contract that lets an issuer attach a jurisdiction
  code (e.g. an ISO 3166-1 alpha-2 country code) to an address. Exposes
  `initialize`, `set_jurisdiction`, `get_jurisdiction` and
  `is_permitted_jurisdiction(address, allowed_codes)`. Setting a code emits a
  `JurisdictionSet` event.
- `denylist-gate-consumer` example crate: a minimal token contract showing how
  to compose `denylist-gate` via a cross-contract `check()` call on both
  parties before mutating balances in `transfer`.

[Unreleased]: https://github.com/stellar-compliance-kit/compliance-primitives/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/stellar-compliance-kit/compliance-primitives/releases/tag/v0.1.0
