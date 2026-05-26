# AGENTS.md - zencan-client crate

## Scope

Applies to `zencan-client/`, the async client library for communicating with
CANopen devices.

## Local Guidance

- Client API shape, request/response correlation, timeout behavior, scanning,
  SDO/PDO helpers, and node configuration loading are user-visible contracts.
  Keep the README, CLI callers, and integration tests in sync with changes.
- Keep async/Tokio boundaries clear. Linux SocketCAN assumptions should remain
  scoped to Linux or socketcan feature paths.
- Error handling should distinguish transport failures, timeouts, protocol
  aborts, parse/config errors, and device responses.
- Configuration TOML schema changes should be checked against
  `zencan-cli load-config` behavior and example configuration files.

## Verification

- Prefer `cargo test -p zencan-client` for crate changes.
- Bus, SocketCAN, or Linux-only transport changes should state whether SocketCAN
  behavior was checked.
- For behavior shared with the CLI, also run `cargo test -p zencan-cli` where
  practical.
