# Changelog

Human-friendly documentation of releases and what's changed in them for the zencan-client crate.

## v0.0.5 - 2026-09-04

### Added

- `SdoClient::read_bool` and `SdoClient::write_bool` helpers
  ([#75](https://github.com/mcbridejc/zencan/pull/75)).
- `SdoClient::set_rpdo_cob_id` and `SdoClient::set_tpdo_cob_id` to change a PDO's COB-ID, enabled state, and
  RTR setting without rewriting its mappings ([#94](https://github.com/mcbridejc/zencan/pull/94)).
- Public `SocketCanSender` and `SocketCanReceiver` types in zencan-common
  ([#86](https://github.com/mcbridejc/zencan/pull/86); thanks to @SebKuzminsky).

### Changed

- **Breaking:** Remove typed `SdoClient::upload_*` and `download_*` accessors; use the corresponding `read_*`
  and `write_*` methods ([#74](https://github.com/mcbridejc/zencan/pull/74)).
- **Breaking:** Move `PdoConfig` communication fields into `comm: PdoCommParameter`. The TOML configuration
  format is unchanged ([#94](https://github.com/mcbridejc/zencan/pull/94)).

### Fixed

- Disable PDOs and clear their mapping count before writing new mappings
  ([#94](https://github.com/mcbridejc/zencan/pull/94)).
- Honor `rtr_disabled` when writing PDO configuration ([#94](https://github.com/mcbridejc/zencan/pull/94)).
- Tighten validation on `ConfiguredNodeId::new`. It now rejects 255 as an invalid configured node ID
  ([#95](https://github.com/mcbridejc/zencan/pull/95)).

## v0.0.4 - 2026-05-08

### Added

- Support for i24 and u24 data types (thanks to @rohel1)

### Changed

- Make socketcan optional to allow building on mac and windows (thanks to @Carbohydrate-42)
- Add `sync` command to send a SYNC object (thanks to @SebKuzminsky)

## v0.0.3 - 2026-01-20

### Added

- Support for TimeOfDay, TimeDifference, f64, u64, i64 access in `SdoClient`

## v0.0.2 - 2025-12-29

### Added

- `SdoClient::set_timeout` method to allow changing the SDO timeout.
- `SdoClient::read_tpdo_config` and `SdoClient::read_rpdo_config` for retreiving PDO configuration
  from a node.
- `SdoClient::block_upload` for transferring large chunks of data from nodes.

### Changed

- Default SDO client timeout changed from 100ms to 150ms.
- NodeConfiguration is moved into `common`.
- The `cob` attribute on `node_configuration::PdoConfig` is renamed to `cob_id`.
- Better error handling on CAN send errors and `BusManager::scan`.

### Fixed

- Bug in `SdoClient` during PDO configuration where CAN ID was masked with `0xFFFFFF` instead of
  `0x1FFFFFF`, so top bit of extended IDs would not be set correctly (#36).
- Fix and retry message sending in SdoClient on failure. With block downloads, it is easy to overrun
  transmit buffers and fail, and the desired behavior is to wait and try again.

## v0.0.1 - 2025-10-09

The first release! 

### Added

- Everything!
