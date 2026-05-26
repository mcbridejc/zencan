# AGENTS.md - zencan-macro crate

## Scope

Applies to `zencan-macro/`, the proc-macro crate re-exported by `zencan-node`.

## Local Guidance

- Macro output is downstream compile-time API. Input syntax, expansion shape,
  and diagnostic changes should be evaluated with `zencan-node` documentation
  and examples.
- Prefer reusing types and generation logic from `zencan-build` and
  `zencan-common` instead of copying protocol layout implementations.
- Compile errors should point as closely as possible to the user input and name
  the relevant field, object, or attribute issue.
- cargo-expand can be useful for debugging, but expanded temporary files should
  not be committed.

## Verification

- Prefer `cargo test -p zencan-macro` for crate changes.
- If macro output affects examples, also run the relevant example build or
  `cargo build --examples --verbose`.
