# AGENTS.md - zencan-cli crate

## Scope

Applies to `zencan-cli/`, including `zencandump` and the interactive
`zencan-cli` shell.

## Local Guidance

- CLI arguments, interactive commands, exit codes, stdout/stderr, shell parsing,
  and completion behavior are user-visible.
- Command semantics changes should update README examples and include tests for
  parsing boundaries, error output, or interactive behavior where practical.
- Process exit codes should indicate success or failure. Business counts should
  be returned through stdout or structured output, not as successful exit codes.
- SocketCAN and network interface operations can affect real systems. Clearly
  distinguish parser/build tests from `vcan` or real interface checks.
- Prefer clap, reedline, and shlex structured parsing facilities over fragile
  hand-written parsers.

## Verification

- Prefer `cargo test -p zencan-cli` for crate changes.
- For CLI compile or Linux-only dependency changes, also run
  `cargo build -p zencan-cli`.
- SocketCAN behavior changes should state whether `vcan0` or a real CAN
  interface was used.
