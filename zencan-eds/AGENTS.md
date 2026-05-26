# AGENTS.md - zencan-eds crate

## Scope

Applies to `zencan-eds/`, the EDS-related helper crate. This crate is currently
`publish = false`.

## Local Guidance

- EDS parsing and generation affect external tool interoperability. Section,
  datatype, index/sub-index, and default value handling changes should preserve
  traceable fixtures.
- Do not describe experimental EDS behavior as a stable published API.
- Logic shared with the object dictionary schema should either converge on
  shared types or keep an explicitly documented boundary to avoid drift from
  `zencan-build`.

## Verification

- Prefer `cargo test -p zencan-eds` for crate changes.
- For input or output format changes, add or update fixtures covering round-trip
  behavior or parser boundaries.
