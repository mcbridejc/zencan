# AGENTS.md - zencan-common crate

## Scope

Applies to `zencan-common/`, the shared protocol foundation used by node,
client, build, and CLI crates.

## Local Guidance

- Types in this crate often propagate across the workspace. Changes to object,
  CAN frame, SDO/PDO/NMT/SYNC, scalar, or error types should be checked against
  `zencan-node`, `zencan-client`, `zencan-build`, and `integration_tests`.
- Preserve `std`, `socketcan`, `log`, and `defmt` feature boundaries. Shared
  types should not accidentally require Linux or Tokio.
- CANopen encoding and decoding, endianness, bit width handling, 24-bit/64-bit
  scalar support, and COB-ID handling are protocol contracts. Add focused tests
  instead of relying only on indirect integration coverage.
- Public re-exports affect downstream ergonomics. Public type moves, renames, or
  error enum changes should be reflected in documentation and callers.

## Verification

- Prefer `cargo test -p zencan-common` for crate changes.
- For feature boundary changes, also run
  `cargo check -p zencan-common --no-default-features`.
- SocketCAN-related changes should state whether the socketcan feature path was
  checked.
