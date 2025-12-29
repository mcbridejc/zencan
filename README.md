# Zencan

![Build](https://github.com/mcbridejc/zencan/actions/workflows/rust.yml/badge.svg)

Easily build and control CANOpen nodes in Rust.

Zencan's goal is to enable rapid creation of a CANOpen node in a `no_std` embedded context using a
TOML configuration file and code generation. It also provides utilities for communicating with the
nodes from a linux environment.

**This project is still in the prototype stage, so it's lacking some features and may yet go through a
lot of API churn.**

## Components

- [`zencan-node`](zencan-node/): Implements a zencan node. `no_std` compatible.
- [`zencan-build`](zencan-build/): Code generation for generating the static data associated with a node, based on a *device config* TOML file.
- [`zencan-client`](zencan-client/): Client library for communicating with nodes
- [`zencan-cli`](zencan-cli/): Command line tools for interacting with devices
- [`zencan-common`](zencan-common/): Shared library used by both node and client

## Why

I like CAN, and I wanted to make it easy to build devices with lots of communication features in Rust -- mostly so I would use that, instead of like, hard-coding that one CAN message I need my device to send.

## Goals

- Support embedded targets with `no_std`/`no_alloc` with statically allocated object storage
- Support enumeration of devices on a bus
- Support software version reporting and bootloading over the bus
- Support CAN-FD
- Support bulk data transfer
- Generate EDS and DBC files for integration into existing tools
- Support persistence of configuration to flash via application provided callbacks

## Example Projects

- [can-io-firmware](https://github.com/mcbridejc/can-io-firmware) - A simple program to read analog inputs and make then available on a CAN bus
- [i4-controller-firmware](https://github.com/mcbridejc/i4-controller-firmware) - A 4-channel current controller

## Building docs

Uses nightly docs features on docs.rs. To build docs locally using nightly features:

```
RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --no-deps
```

## License

This project is licensed uder the [MPL-2.0](LICENSE) license.
