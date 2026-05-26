# AGENTS.md - integration_tests

## Scope

Applies to `integration_tests/`, the workspace integration tests for combined
node, client, and build behavior.

## Local Guidance

- These tests should cover cross-crate contracts. Keep pure single-crate unit
  boundaries in the owning crate where possible.
- Test names should describe protocol behavior clearly, such as PDO mapping,
  SYNC-triggered transmission, SDO aborts, lifecycle behavior, or configuration
  loading.
- When adding several related regression cases, run the full relevant test
  target if Cargo's name filtering would skip cases unintentionally.
- Async tests, `serial_test`, and randomized data should remain reproducible and
  avoid reliance on execution order or real CAN hardware.

## Verification

- Prefer `cargo test -p integration_tests` for changes in this directory.
- For targeted regressions, also run the relevant test name when practical.
