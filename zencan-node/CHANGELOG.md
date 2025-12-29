# Changelog

Human-friendly documentation of releases and what's changed in them for the zencan-node crate.

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
