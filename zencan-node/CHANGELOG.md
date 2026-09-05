# Changelog

Human-friendly documentation of releases and what's changed in them for the zencan-node crate.

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
