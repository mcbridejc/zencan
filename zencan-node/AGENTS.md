# AGENTS.md - zencan-node crate

## Scope

Applies to `zencan-node/`, the embedded-first CANopen node implementation.

## Local Guidance

- `no_std` / `no_alloc`, static object storage, and MCU use are core constraints
  for this crate. Avoid introducing heap use, blocking OS assumptions, or
  default `std` requirements without a clear compatibility reason.
- Keep `std`, `log`, `defmt`, and `socketcan` feature boundaries clear. Default
  feature convenience must not break the `default-features = false` embedded
  path.
- NMT state, the SDO server, PDO mapping and transmission, SYNC handling, object
  access rights, and storage callbacks are protocol-sensitive paths. Cover
  boundary conditions with focused tests.
- Interface changes to the generated object dictionary must be evaluated with
  `zencan-build/` and `zencan-macro/`.

## Verification

- Prefer `cargo test -p zencan-node` for crate changes.
- For feature or `no_std` boundary changes, also run
  `cargo check -p zencan-node --no-default-features`.
- SocketCAN-related changes should cover `examples/socketcan_node/` or clearly
  state that only non-SocketCAN checks were run.
