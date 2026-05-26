# AGENTS.md - examples

## Scope

Applies to all examples under `examples/`. More specific example directories can
supplement this guidance with their own `AGENTS.md`.

## Local Guidance

- Examples are user-facing learning paths. Public workflow, CLI command, device
  config, or wiring changes should update the matching example README.
- Examples should demonstrate the recommended repository usage and avoid
  bypassing the normal `zencan-build` / `zencan-node` generation and include
  paths.
- Do not commit build output, flash logs, temporary expanded/generated files, or
  local hardware configuration.

## Verification

- For the Linux example, prefer `cargo build -p socketcan_node` or the workspace
  examples build.
- For embedded examples, prefer `cargo build` from the specific example
  directory and state whether the change was only built or also run on hardware.
