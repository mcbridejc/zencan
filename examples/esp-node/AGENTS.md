# AGENTS.md - esp-node example

## Scope

Applies to `examples/esp-node/`, the ESP32C3 dummy CANopen node.

## Local Guidance

- This directory is its own Cargo workspace and is not a member of the root
  workspace. Keep that boundary clear when changing dependencies, profiles,
  target configuration, or build scripts.
- The README documents `espflash`, USB-UART, CAN bitrate, and host CAN interface
  steps. Treat build, flash, board-info, and CAN bus checks as separate
  verification layers.
- `zencan-node` is used with `default-features = false`. Avoid accidentally
  introducing `std` or breaking the embedded path.

## Verification

- Prefer `cargo build` from this directory for code changes.
- Only report flash or hardware validation when an ESP32C3 was actually used for
  `cargo run` or `espflash board-info`.
