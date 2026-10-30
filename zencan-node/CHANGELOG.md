# Changelog

Human-friendly documentation of releases and what's changed in them for the zencan-node crate.

## Unreleased

### Added

- CAN FD support: nodes exchanging frames of up to 64 bytes, with PDOs of up to 64 bytes. See
  [docs/canfd.md](../docs/canfd.md).
  - `CanFdMessage`, the CAN FD frame type (zencan-common), with BRS and ESI flags, and the `CanFrame`
    trait implemented by `CanMessage` and `CanFdMessage`, with `fd_padded_len` and `TryFrom`
    conversions between the two.
  - A `[can] frame_format = "classic" | "fd"` device config setting (zencan-build). The generated
    module exports the selected frame type as `Frame`. CAN FD PDO defaults may map up to 64 bytes
    and must set `rtr_disabled = true`, since CAN FD has no remote frames.
  - `open_socketcan_fd`, a CAN FD socketcan backend (zencan-common, re-exported here).
  - `ClassicAdapter` (zencan-common), to run classic CAN code such as the zencan-client masters on a
    CAN FD transport.
  - The sealed `NodeFrame` trait, implemented for `CanMessage` and `CanFdMessage`, and `PdoBuffer`.
  - A CAN FD build of the stm32l5-minimal example (`--features fd`).

  CAN FD is selected in the device config rather than with a Cargo feature. A type which is never
  instantiated costs no RAM or flash, so keeping the two frame types separate is enough to spare
  classic nodes any cost. Cargo features are unified across the dependency graph, so a feature would
  have turned every node of a workspace into a 64-byte one as soon as one node used CAN FD. The
  device config is per node, and already drives code generation.

### Changed

- `NodeMbox`, `Node`, `NodeState`, `Pdo`, `PdoData`, `PdoCommObject` and `PdoMappingObject` take a
  frame type parameter, and the CAN transport traits (`CanSender`, `CanReceiver`, `AsyncCanSender`,
  `AsyncCanReceiver`, `CanSendError`) take a frame type parameter. All default to `CanMessage`, so
  classic CAN code is unaffected.
- **Breaking:** `PdoData::buffered_value` holds a `PdoBuffer<8>` instead of a
  `heapless::Vec<u8, 8>` for classic CAN. It dereferences to the payload bytes as before, and saves
  4 bytes of RAM per PDO.
- **Breaking:** `PdoData::new()` constructs classic CAN storage only; `PdoData::default()` is
  generic over the frame type.
- **Breaking:** `DeviceConfig` (zencan-build) has a new `can` field.
- TPDOs are read straight into their payload buffer, without an intermediate stack array.
- The SDO server mailbox and `BlockSegment` parsing accept frames longer than 8 bytes, using the
  first 8, so that SDO messages padded in CAN FD frames are understood.
- zencan-common requires tokio 1.53.3 or later, for `AsyncFd::register`.

### Fixed

- Deprecation warning from tokio 1.53.3 in the socketcan backend (`AsyncFd::new`).
- Broken intra-doc links.

## v0.0.5 - 2026-09-04

### Added

- `Callbacks::pdo_received` callback with the RPDO slot index and mapping entries
  ([#88](https://github.com/mcbridejc/zencan/pull/88); thanks to @rohel01).
- `ObjectAccess::read_u24` and `ObjectAccess::read_i24` helpers
  ([#88](https://github.com/mcbridejc/zencan/pull/88); thanks to @rohel01).
- `SubInfo::new_boolean` helper in zencan-common ([#77](https://github.com/mcbridejc/zencan/pull/77); thanks
  to @rohel01).

### Changed

- TPDO frames now use the mapped data length instead of always sending eight bytes
  ([#79](https://github.com/mcbridejc/zencan/pull/79)).
- **Breaking:** Rename `SubInfo::new_visibile_str` to `SubInfo::new_visible_str`
  ([#78](https://github.com/mcbridejc/zencan/pull/78)).
- Refresh ESP node example dependencies and add ESP32-C3 / ESP32-C6 target selection
  ([#91](https://github.com/mcbridejc/zencan/pull/91); thanks to @etiennedm).

### Fixed

- Boolean object code generation and construction of `ScalarField<bool>`
  ([#75](https://github.com/mcbridejc/zencan/pull/75)).
- `NodeMbox::store_message` incorrectly returned an error for handled SDO requests and did not call
  `process_notify` ([#83](https://github.com/mcbridejc/zencan/pull/83); thanks to @rohel01).
- Set the NMT state before calling `reset_app` and `reset_comms`, allowing callbacks to restore PDO mappings
  ([#93](https://github.com/mcbridejc/zencan/pull/93)).
- `ConfiguredNodeId::new` now rejects 255 as an invalid configured node ID
  ([#95](https://github.com/mcbridejc/zencan/pull/95)).

## v0.0.4 - 2026-05-08

### Added

- `sync_received` callback (thanks to @rohel1)
- Support for i24 and u24 data types (thanks to @rohel1)

### Changed

- embedded_io bumped from 0.6 to 0.7

### Added

- `sync_received` node callback added (thanks to rohel01)

### Fixed

- Bug sending TPDO on SYNC received (PR #66)
- Unused code warning when num_tdpos or num_rpdos is 0
- TPDO bug where event flags were never cleared causing all TPDOs to be transmitted when any event
  was set
- PDO objects were unwritable when reset_app/reset_comms callback is called
- Objects with `application_callback` failed to compile
- CallbackObject provided no method for registering a callback

## v0.0.3 - 2026-01-20

### Added

- Support for TimeOfDay, TimeDifference, f64, u64, i64 object data types (#42).
- Device config `autostart` field for configuring 0x5000 object default value (#47).

### Fixed

- Fix record sub object accessor/field naming to use decimal instead of hex values (e.g. `get_sub10` instead of `get_suba`).

## v0.0.2 - 2025-12-29

### Added

- Default initialization of PDO configuration in device config (#36)
- Callbacks added for `ResetApp`, `ResetComms`, `EnterPreoperational`, `EnterOperational`,
  `EnterStopped`.
- Support for SDO block upload.

### Changed

- Callbacks restructured to be passed by `Callbacks` object upon Node creation, and to support
  non-static lifetime (#36).
- Outgoing messages are queued and passed via NodeMbox, switching to a "pull" for the application, with a notification callback when new messages are queued.

## v0.0.1 - 2025-10-09

The first release! 

### Added

- Everything! 
