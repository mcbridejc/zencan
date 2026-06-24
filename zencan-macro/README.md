# zencan-macro

Crate containing proc-macros for zencan. These are re-exported by `zencan-node`, so you probably do
not need to depend on this crate directly.

## Debugging Hints

Cargo expand is useful for seeing the macro output:

`cargo install cargo-expand`
`cargo expand --example record`

When the output generates compile errors, you can do the following to get better errors:

`cargo expand --example record > examples/expanded.rs`
`cargo build --example expanded`


