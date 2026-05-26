# AGENTS.md - zencan

## Scope

This file applies to the whole repository. More specific `AGENTS.md` files in
subdirectories apply to their directories and supplement this guidance.

## Project Structure

- `zencan-node/`: CANopen node implementation for embedded `no_std` /
  `no_alloc` use with statically allocated object storage.
- `zencan-build/`: code generation from device config TOML into the static
  object dictionary used by `zencan-node`.
- `zencan-common/`: shared protocol, object, scalar, error, and CAN frame types.
- `zencan-client/`: async client library for communicating with CANopen devices,
  including Linux SocketCAN support.
- `zencan-cli/`: `zencandump` and the interactive `zencan-cli` tools.
- `zencan-macro/`: proc-macro crate re-exported by `zencan-node`.
- `zencan-eds/`: EDS-related helper crate; currently `publish = false`.
- `integration_tests/`: workspace-level integration tests across node, client,
  and build behavior.
- `examples/`: Linux SocketCAN, ESP32C3, and STM32G0 example nodes.
- `.github/workflows/rust.yml`: source of truth for the current CI checks.

## Tooling

- Treat `rust-toolchain.toml`, the Cargo workspace, crate README files, example
  README files, and `.github/workflows/rust.yml` as the repository truth
  surfaces for toolchain, build, test, and documentation expectations.
- The root workspace uses Rust edition 2021 and the pinned toolchain from
  `rust-toolchain.toml`. Do not change the edition, MSRV, or toolchain without
  an explicit compatibility reason.
- Keep each crate's existing `std` / `no_std`, `log` / `defmt`, and `socketcan`
  feature boundaries intact.

## Protocol And API Expectations

- CANopen wire behavior, object dictionary layout, PDO/SDO/NMT/SYNC semantics,
  scalar encoding, and COB-ID handling are protocol contracts. Changes should
  include focused tests and clear verification notes.
- Prefer existing structured configuration, TOML parsing, object dictionary, and
  code generation paths over ad hoc string manipulation.
- Avoid using `.clone()`, `unwrap()`, or `expect()` as default fixes. Check the
  ownership model, error boundary, and embedded resource constraints first.
- Unsafe code should include a local `// SAFETY:` rationale and keep unsafe
  blocks as small as practical.

## Verification

- Repository-wide Rust changes should align with CI where practical:
  - `cargo build --examples --verbose`
  - `cargo build` in `examples/esp-node`
  - `cargo clippy -- -D warnings`
  - `cargo clippy --examples -- -D warnings`
  - `cargo fmt --check`
  - `cargo test --verbose`
  - `cargo build` in `examples/stm32g0-lilos-node`
- For scoped changes, prefer the smallest command that covers the affected
  crate or example, then report what was run.
- Distinguish host tests, SocketCAN/vcan checks, embedded builds, flashing, and
  real CAN hardware validation. Do not present one evidence layer as another.

## Documentation

- Update the relevant crate README or example README when user-visible API, CLI,
  configuration format, protocol behavior, example workflow, or feature
  boundaries change.
- Do not commit build output, generated debug artifacts, `target/` contents, or
  local editor/assistant state.
