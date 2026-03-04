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

## Contributing

### Pull Requests

If you're interested in using zencan, that's great! Given its early state and broad goals, it's likely you will come across a need to extend it. 
PRs are welcome! Some things are straight-forward bug fixes or feature extensions, and you can feel free to land a surprise PR. If you're embarking on a 
bigger project, feel free to reach out and talk about what you're trying to do and perhaps we can gain some efficiency by coordinating. My email 
is open, as are the [project discussions](https://github.com/mcbridejc/zencan/discussions).

### Opening Issues

If you find a bug, feel free to open an issue. If that issue comes with a PR to fix it, all the better, but even if not I am interested in 
the reports as well. One thing I do ask is to please limit issues to actionable topics. I don't want to use issues for questions about how to 
use the library, or feature ideas, etc. For that, please use [discussions](https://github.com/mcbridejc/zencan/discussions), or contact me 
directly. 

### General Feedback

If you use zencan, I'd love to hear about it. If it doesn't quite meet your use-case, I'd love to hear why that is too. Drop me an email or 
post a discussion thread. 

## Building docs

Uses nightly docs features on docs.rs. To build docs locally using nightly features:

```
RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --no-deps
```

## License

This project is licensed uder the [MPL-2.0](LICENSE) license.
