# AGENTS.md - zencan-build crate

## Scope

Applies to `zencan-build/`, the library that generates object dictionary Rust
code from device config TOML.

## Local Guidance

- Generated code is the compile-time contract consumed by `zencan-node`. Schema,
  default value, object/subobject layout, PDO mapping, and access type changes
  should be evaluated against node runtime behavior and documentation examples.
- Prefer extending the existing parser, generated syntax tree, quote logic, and
  test fixtures over hand-built Rust source strings.
- Error messages should help locate the relevant config field, index/sub-index,
  or type issue instead of only exposing lower-level `syn`, `prettyplease`, or
  rustfmt errors.
- The README debugging workflow can create temporary files such as `temp.rs`;
  do not commit those artifacts.

## Verification

- Prefer `cargo test -p zencan-build` for crate changes.
- For generated syntax or formatting changes, generate and compile output from
  existing examples or tests. `cargo run -p zencan-build --example build_od -- <config>`
  can be used to inspect generated code when diagnosing failures.
