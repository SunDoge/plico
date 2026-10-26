# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3](https://github.com/SunDoge/plico/compare/v0.1.2...v0.1.3) - 2026-10-26

### Added

- *(ui)* add hover responses and cached network animation gallery

### Fixed

- *(fmt)* join braces after packing if conditions

### Other

- *(deps)* bump cairo-rs from 0.18.5 to 0.22.9
- *(deps)* use upstream glifo without local patches

## [0.1.2](https://github.com/SunDoge/plico/compare/v0.1.1...v0.1.2) - 2026-10-26

### Added

- *(input)* support scoped shortcuts and system global hotkeys

## [0.1.1](https://github.com/SunDoge/plico/compare/v0.1.0...v0.1.1) - 2026-10-26

### Fixed

- *(text)* honor per-node generic font families

## [0.1.0](https://github.com/SunDoge/plico/releases/tag/v0.1.0) - 2026-10-25

### Added

- add synchronous component state and host tasks
- *(cli)* a bundle command for release application directories
- *(lvgl)* add host and ESP32-P4 experiments with CI
- *(cli)* pack multiline argument lists in component formatting
- initialize Plico immediate-mode GUI workspace

### Other

- [**breaking**] simplify app package names and rename terminal library
- *(deps)* bump tree-sitter from 0.25.10 to 0.27.1
- [**breaking**] name GUI backends and separate the web demo
