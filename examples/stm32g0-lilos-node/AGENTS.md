# AGENTS.md - stm32g0-lilos-node example

## Scope

Applies to `examples/stm32g0-lilos-node/`, the STM32G0B1 + lilos async runtime
example node.

## Local Guidance

- This directory is its own Cargo workspace with its own `rust-toolchain.toml`
  and edition 2024. Do not apply the root workspace toolchain or edition
  assumptions here without checking the local files.
- `defmt`, RTT, panic-probe, cortex-m runtime, FDCAN, and STM32 peripheral setup
  are embedded target boundaries. Keep startup, interrupt, memory, and profile
  changes explicit.
- `zencan-node` is used with `default-features = false` and `defmt`. Avoid
  introducing `std` or Linux-only dependencies.

## Verification

- Prefer `cargo build` from this directory for code changes.
- Distinguish compile verification from probe, board, RTT, flash, or CAN bus
  validation.
