# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.10](https://github.com/SunDoge/plico/compare/v0.1.9...v0.1.10) - 2026-10-28

### Added

- *(render)* make Skia the default and split Vello backend

### Other

- separate plico core and desktop entry points

## [0.1.9](https://github.com/SunDoge/plico/compare/v0.1.8...v0.1.9) - 2026-10-28

### Added

- add plico application facade
- *(native)* add screenshot and floating overlay foundations

### Fixed

- *(shell)* avoid activating floating overlays initially

## [0.1.8](https://github.com/SunDoge/plico/compare/v0.1.7...v0.1.8) - 2026-10-28

### Added

- *(egui)* host the egui backend in a window and run the settings app
- *(egui)* replay SVG and box shadows; share the usvg projector

### Fixed

- *(core)* mark viewport dirty on height-only resize
- *(skia)* gate SVG error conversion behind raster feature

### Other

- *(egui)* use native controls and text with Taffy layout

## [0.1.7](https://github.com/SunDoge/plico/compare/v0.1.6...v0.1.7) - 2026-10-27

### Added

- *(video)* support persistent streaming playback and media overlays

### Fixed

- *(shell)* synchronize window resizing and frame presentation

## [0.1.6](https://github.com/SunDoge/plico/compare/v0.1.5...v0.1.6) - 2026-10-27

### Added

- *(ui)* add table rows and property inspection components
- *(ui)* add reusable avatar and ruby text components
- *(window)* expose maximum size and custom resize edges

### Fixed

- *(skia)* preserve fractional text motion on GPU surfaces
- *(ci)* sync main release version and format merged sources

### Other

- *(core)* borrow cached image encoding during declarations
- *(renderer)* bound caches and add optional sparse damage rendering
- *(diagnostics)* profile flatten and retained backend phases
- *(skia)* instrument lyric rendering phases and document measurements

## [0.1.5](https://github.com/SunDoge/plico/compare/v0.1.4...v0.1.5) - 2026-10-26

### Added

- *(skia)* use SkParagraph for ordinary text layout and painting
- *(skia)* support optional incremental presentation hints
- *(skia)* track damage per preserved render target
- *(skia)* add optional damage-clipped surface drawing
- *(runtime)* measure per-frame damage coverage (damage tracking phase 0)

### Fixed

- *(layout)* clear hoisted children when containers become leaves
- *(text)* reuse live font blobs after source cache pruning
- *(renderer)* preserve local clips before presentation transforms
- *(diagnostics)* report actual renderer damage coverage
- *(runtime)* avoid resettling unchanged async service snapshots
- *(runtime)* keep the geometry latch set and stop damage from reading it
- *(runtime)* reset geometry_dirty once its full flatten consumed it
- *(skia)* share collection font data and release stale typefaces
- *(skia)* make automatic damage tracking explicitly opt-in
- *(skia)* render image brushes and share target resources

### Other

- *(skia)* skip drawing commands outside damage
- *(diagnostics)* log the active Skia damage mode
- *(vello)* probe preserved-target partial redraw support
- *(skia)* isolate translucent layer damage precision mismatch

## [0.1.4](https://github.com/SunDoge/plico/compare/v0.1.3...v0.1.4) - 2026-10-26

### Added

- *(motion)* add analytic spring flights to the Animated facade
- *(motion)* add an Animated scalar facade for app-driven tweens

### Fixed

- *(motion)* align Animated.active with value's elapsed clock and drop dead state
- *(renderer)* compare observable image-surface state instead of Arc identity

### Other

- *(image)* cache encoded assets and reuse borrowed cache keys
- *(text)* avoid layout invalidation for color-only spans

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
