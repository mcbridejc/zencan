# Changelog

Human-friendly documentation of releases and what's changed in them for the zencan-cli crate.

## v0.0.5 - 2026-09-04

### Added

- Accept hexadecimal node IDs (such as `0x10`) in node-targeted commands, including `nmt` and `lss
  set-node-id` ([#90](https://github.com/mcbridejc/zencan/pull/90); thanks to @etiennedm).

### Fixed

- Accept negative values in the `write` command ([#80](https://github.com/mcbridejc/zencan/pull/80)).

## v0.0.4 - 2026-05-08

### Added

- SYNC send support to BusManager (thanks to @SebKuzminsky)

## v0.0.3 - 2026-01-20

### Changed

- Update zencan-client to v0.0.3

## v0.0.2 - 2025-12-29

### Fixed

- Panic during scan when socketcan fails to send messages

## v0.0.1 - 2025-10-09

The first release! 

### Added

- Everything!
