# AGENTS.md - socketcan_node example

## Scope

Applies to `examples/socketcan_node/`, the Linux SocketCAN example node.

## Local Guidance

- This is the primary example for node/client/CLI interoperability on Linux.
  Bus wiring, node object config, PDO/SDO behavior, and logging changes should
  be considered with `zencan-cli` documentation.
- SocketCAN behavior depends on a Linux CAN interface. Distinguish compile
  checks from `vcan0` or real CAN interface validation.
- Keep the example clear and readable. Avoid mixing test-only fixture behavior
  into the user-facing example.

## Verification

- Prefer `cargo build -p socketcan_node` for changes in this example.
- When SocketCAN is available, a manual smoke with `vcan0` and
  `zencandump` / `zencan-cli` is the relevant bus-level check.
