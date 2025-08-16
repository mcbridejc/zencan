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

[can-io-firmware](https://github.com/mcbridejc/can-io-firmware) - A simple program to read analog inputs and make then available on a CAN bus

## Building docs

Uses nightly docs features on docs.rs. To build docs locally using nightly features:

```
RUSTDOCFLAGS="--cfg docsrs" cargo +nightly doc --no-deps
```

## Some TODOs

No doubt this is a very incomplete list...

- Support for multiple SDO servers
- Support for SYNC driven RPDO
- EDS generation
- Direct response in receive IRQ
    - Should support an optional immediate TX message queuing for fast turnaround, e.g. on LSS
      fastscan messages, regardless of process timing. It isn't required, but could be implemented
      on a lot of systems.
- Non-async API for zencan-client
- EMCY service
  - Send EMCY message on receiving PDO of invalid length
- zencandump needs to display COB-ID, e.g. for SdoRequest messages to distinguish nodes or servers
- integration test wrapper
  - DRY up the node process task creation
  - print returned errors with Display
- Add better validity checking on PDO configuration
  - Don't allow changing things while operating
- zencan-build should maybe be exported and documented via zencan-node (?)
  - This requires new message sending semantics with pushback to support sending segments as fast as
    possible while prioritizing other messages
- Support object value range limits: return AbortCode if SDO client attempts to write out of range value
- SDO buffer size should be configurable
- SDO segmented downloads should use buffer so object access is atomic
- Building a node example with a no_std target needs to be part of CI test
- DeviceConfig and complete object dictionary representations should be separated. They are separate
  concepts: the device config is an input file for configuring node generation. Separately, there is
  a dictionary that is more analagous to the EDS which includes all of the auto generated objects as
  well as the application specific objects defined in the device config.

## License

This project is licensed uder the [MPL-2.0](LICENSE) license.
