# Changelog

Human-friendly documentation of releases and what's changed in them for the zencan-node crate.

## Unreleased

### Added

- Default initialization of PDO configuration in device config (#36)
- Callbacks added for `ResetApp`, `ResetComms`, `EnterPreoperational`, `EnterOperational`,
  `EnterStopped`
### Changed

- Callbacks restructured to be passed by `Callbacks` object upon Node creation, and to support
  non-static lifetime (#36)

## v0.0.1 - 2025-10-09

The first release! 

### Added

- Everything! 
