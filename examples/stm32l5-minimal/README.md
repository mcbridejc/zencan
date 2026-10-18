# STM32L5 minimal zencan node

Minimal firmware for a zencan node on STM32L5 family.

The default build is a classic CAN node, configured by `zencan_config.toml`, and is the build
tracked for binary size.

Building with `--features fd` generates the node from `zencan_config_fd.toml` instead, which selects
CAN FD: the FDCAN peripheral is configured for CAN FD with bit rate switching (500 kbit/s
arbitration, 1 Mbit/s data phase), and the node sends a 64-byte TPDO. The CAN driver in `src/can.rs`
is the same for both builds: it is written against the `zencan::Frame` type the generated code
exports.

```sh
cargo build --release --features fd
```
