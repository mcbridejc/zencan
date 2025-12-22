# Changelog

Human-friendly documentation of releases and what's changed in them for the zencan-client crate.

## [Unreleased]

### Added

- `SdoClient::set_timeout` method to allow changing the SDO timeout
- `SdoClient::read_tpdo_config` and `SdoClient::read_rpdo_config` for retreiving PDO configuration
  from a node

### Changed

- Default SDO client timeout changed from 100ms to 150ms
- NodeConfiguration is moved into `common`
- The `cob` attribute on `node_configuration::PdoConfig` is renamed to `cob_id`.
- Better error handling on CAN send errors and `BusManager::scan`.

### Fixed

- Bug in `SdoClient` during PDO configuration where CAN ID was masked with `0xFFFFFF` instead of
  `0x1FFFFFF`, so top bit of extended IDs would not be set correctly (#36).

## [v0.0.1] - 2025-10-09

The first release! 

### Added

- Everything! 