# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.2](https://github.com/SunDoge/plico/compare/v0.3.1...v0.3.2) - 2026-11-09

### Added

- *(widgets)* share materials and add MyGO-inspired Rust components
- *(csharp)* add reusable liquid glass and managed widget toolkit
- *(runtime)* add MyGO-style editable text selection with native visual lines
- *(native)* add system-backed vertical document text
- *(shell)* follow macOS accessibility display preferences
- *(materials)* prefilter backdrop blur with bounded mip pyramids
- *(renderer)* match shadow contours to continuous widget corners
- *(widgets)* add clipped inset shadows across native renderers
- *(widgets)* animate glass feedback and default widgets to continuous corners
- *(window)* toggle maximization using native window state
- *(window)* support macOS titlebar overlay with native controls
- *(csharp)* add progressive scroll-edge and backdrop blur
- *(renderer)* adapt MyGO quartic glass refraction model
- *(gallery)* add Mines gallery and reusable secondary activation
- *(runtime)* notify hosts of meaningful user activity
- *(preview)* add reusable recipe sessions and typed test plans
- *(csharp)* add renderer-backed SDK with intercepted components
- *(shell)* follow Linux portal display preferences
- *(desktop)* provide reusable native tray and actionable notification services

### Fixed

- *(macos)* match AppKit system font and monospace selection
- *(macos)* support private CoreText application fonts
- *(shell)* honor macOS custom titlebar double-click actions
- *(input)* follow macOS multi-click interval in native text selection
- *(shell)* use native continuous window corners on macOS
- *(runtime)* retain coherent presentations and reuse animated text rasters
- *(renderer)* preserve backdrop alpha outside glass contours
- *(renderer)* reuse backdrop targets reserved during frame preflight
- *(ci)* declare built-in shader assets before test modules
- *(renderer)* give liquid glass correct surface refraction
- *(renderer)* preserve canvas drags on identical overlay updates
- *(runtime)* refresh virtual rows after viewport changes
- *(csharp)* restore virtual tables and route game keyboard input
- *(desktop)* coordinate macOS transaction presentation and resize pacing
- *(shell)* use callback timing for macOS display-link resume pacing
- *(backend)* prepare surface before native titlebar double-click minimization
- *(backend)* drain surface acquisition before host visibility changes
- *(shell)* recover Linux preferences across portal restarts
- *(shell)* prepare complete frames before macOS window exposure
- *(shell)* restore minimized windows when shown by native integrations
- *(terminal)* preserve terminal blink phase across event loop pauses

### Other

- *(dev)* reduce application optimization and debug information
- *(text)* compare AppKit font selection using CoreText PostScript names
- *(text)* reuse immutable CoreText baseline attributes per cached face
- *(text)* avoid redundant CoreText encoding index allocations
- *(backend)* add focused macOS native verification entrypoint
- *(backend)* reuse immutable CoreGraphics color space across text rasters
- *(backend)* retain hot CoreText faces across font-size animations
- *(wgpu)* skip retained static frames before drawable waits
- *(renderer)* localize backdrop damage and batch independent snapshots
- *(widgets)* sleep between toast animations and expiration
- *(runtime)* follow precise scrolling and sleep between UI deadlines
- *(wgpu)* borrow prepared draw streams throughout renderer traversal
- *(backend)* configure native CoreText in macOS HUD fixtures
- *(backend)* pause continuous animation while waiting for a drawable
- *(macos)* avoid reapplying native window style during resize
- *(backend)* resume paused macOS animations without an extra display tick
- *(backend)* prefetch macOS surface textures without blocking ordinary frames
- *(backend)* avoid presenting background redraws to invisible windows
- *(desktop)* coalesce native resize frames and trim idle Linux caches
- *(shell)* pace macOS animations with native display links
- *(wgpu)* release idle macOS drawable and frame resources

## [0.3.1](https://github.com/SunDoge/plico/compare/v0.3.0...v0.3.1) - 2026-11-07

### Added

- *(runtime)* support sticky top headers
- *(widgets)* port MyGO color well
- *(widgets)* port MyGO breadcrumbs, meter, spinner and rating
- *(widgets)* add dual-thumb range slider
- *(widgets)* add focus traversal and number input
- *(widgets)* port MyGO alert dialog
- *(widgets)* port MyGO find bar
- *(widgets)* port MyGO radios, link and history buttons
- *(widgets)* port MyGO sidebar and search field
- *(widgets)* port MyGO stepper and split panes

### Fixed

- *(widgets)* scope edit and interaction state to control lifetimes

## [0.3.0](https://github.com/SunDoge/plico/compare/v0.2.1...v0.3.0) - 2026-11-05

### Added

- *(cli)* embed tagged WGSL formatter and format shaders
- *(renderer)* default to wgpu and minimize Vello maintenance
- *(ui)* add settings composition and polish gallery workspace
- *(wgpu)* measure and cache native CoreText intrinsic widths
- *(wgpu)* connect retained CoreText paragraphs to native rendering
- *(native)* expose CoreText run-based selection and paragraph carets
- *(native)* align CoreText paragraphs with native bidi-aware offsets
- *(native)* compose CoreText multiline geometry and paragraph hit testing
- *(native)* retain CoreText soft-wrapped lines with local editing offsets
- *(native)* apply CoreText tracking to retained line geometry
- *(native)* support normalized CoreText face weights
- *(native)* select CoreText bold and italic faces
- *(native)* bound CoreText line rasterization to visible clips
- *(native)* rasterize retained CoreText lines into bounded RGBA images
- *(native)* add bounded CoreText single-line geometry foundation
- *(wgpu)* register private Windows memory fonts with retained loader guards
- *(wgpu)* clip native Windows paragraphs without reshaping visible lines
- *(wgpu)* render native Windows single-line ellipsis
- *(wgpu)* preserve native Windows rich text foreground ranges
- *(wgpu)* support bounded native Windows font style spans
- *(wgpu)* apply native Windows character spacing before measurement
- *(native)* raster DirectWrite outline color glyph layers
- *(wgpu)* support native DirectWrite paragraph line height
- *(wgpu)* preserve native paragraph alignment through editing and paint
- *(native)* resolve Windows system font preferences explicitly
- *(native)* expose DirectWrite intrinsic paragraph widths
- *(native)* correct and colorize DirectWrite grayscale coverage
- *(native)* raster DirectWrite layouts into bounded grayscale run masks
- *(native)* add DirectWrite layout and UTF-8 editing geometry foundation
- *(wgpu)* clip native paragraphs by visible line count
- *(wgpu)* support private application fonts and family aliases
- *(wgpu)* render native rich paragraph ranges with shared geometry
- *(wgpu)* apply glyph-local affine transforms without shifting layout
- *(wgpu)* preserve normalized variable-font instances in glyph runs
- *(native)* raster positioned glyphs from shared font resources
- *(wgpu)* [**breaking**] replay shared scenes with native text and damage
- *(wgpu)* add quad renderer and native text window prototypes
- *(shaders)* let plico_effect read its backdrop, mygo-style
- *(shader)* pack named scalar and vector parameters from WGSL layouts
- *(wgpu)* filter isolated layers with GPU Gaussian blur
- *(wgpu)* render shared WGSL effects with local damage
- *(wgpu)* use native paragraph geometry for text input
- *(gallery)* adopt grouped navigation and shared desktop components
- *(widgets)* animate toast presence and preserve exit lifetimes
- *(widgets)* add mygo-style tabs, toasts, toolbar and drag surfaces
- *(bevy)* embed Plico UI in shared GPU 3D scenes
- *(selection)* add word and paragraph gestures with native highlights
- *(shader)* embed WGSL modules and label source diagnostics
- *(wgpu)* present in the browser and profile damage windows

### Fixed

- *(backend)* support CoreText rich text and ellipsis
- *(wgpu)* rasterize synthetic emboldening in positioned glyph runs
- *(native)* allow monochrome runs in color-capable Windows fonts
- *(wgpu)* infer native Windows paragraph base direction
- *(ci)* repair WASM shader checks and avoid optimized CI builds
- *(wgpu)* correct backdrop effects and hosted shader DPI scaling
- restore the i18n dependency section and simplify filter matching
- *(web)* finish backend removal and check split host in CI
- *(shader)* reject uniform layouts incompatible with float parameters
- *(renderer)* keep additive damage local and invalidate material caches
- *(renderer)* validate WGSL fragment output contract
- *(renderer)* reject unsupported WGSL vertex buffer inputs
- *(wgpu)* synchronize native measurement across zoom and retained layout caches
- *(renderer)* reject nested WebGL effects before replay
- *(runtime)* preserve scrolling during unrelated layout animations
- *(runtime)* bound scene history during rapid theme reversals
- *(shell)* export performance telemetry at window shutdown

### Other

- optimize dependency builds and document the dev probe
- *(native)* reuse bounded CoreText font objects across layouts
- *(wgpu)* cull offscreen DirectWrite paragraphs before rasterization
- *(wgpu)* bound paragraph payloads and reclaim derived text caches
- *(desktop)* add paced software adapter probe and comparison notes
- *(wasm)* un-gate the wgpu effect stack on the browser target
- drop dependencies left behind by the backend removals
- [**breaking**] remove the egui native-widget backend
- [**breaking**] remove the Vello backend and retire wabou naming
- *(renderer)* make native Vello SVG projection opt-in
- *(renderer)* isolate Vello shader texture registration in its adapter
- *(renderer)* verify authored WGSL parameter ABI
- *(wgpu)* reuse layer targets and prune undamaged draws

## [0.2.1](https://github.com/SunDoge/plico/compare/v0.2.0...v0.2.1) - 2026-11-04

### Added

- *(skia)* support SkSL variants for authored shader effects

## [0.2.0](https://github.com/SunDoge/plico/compare/v0.1.11...v0.2.0) - 2026-11-03

### Added

- *(app)* make Immediate the default application path
- *(cli)* make reactive regions the starter default
- *(regions)* unify typed service hosting
- *(runtime)* add incremental reactive backend updates
- *(renderer)* add editable retained image canvas
- *(regions)* add equality-checked derived state
- *(regions)* bind button properties and one-shot actions

### Fixed

- *(skia)* support gaussian backdrop filters
- *(skia)* preserve nested fragment damage correctness
- *(skia)* keep AA commands whole in tracked damage
- *(runtime)* harden incremental regions and backend invalidation
- *(skia)* render synthetic glyphs and complex SVGs

### Other

- *(dev)* [**breaking**] replace desktop hot reload with scenario previews
- *(core)* split user and host view contracts
- *(skia)* remove redundant SVG projection path
- *(runtime)* split adapter source and view modules
- *(launcher)* migrate interactions to regions
- *(regions)* format renamed imports
- *(regions)* rename watch API to region
- *(skia)* enable damage tracking by default
- *(skia)* skip unchanged presents by default
- *(skia)* prototype vertical CJK text compositing
- *(skia)* align render probes and simplify scene history
- *(skia)* paint translated paragraphs without matrix scopes

## [0.1.11](https://github.com/SunDoge/plico/compare/v0.1.10...v0.1.11) - 2026-10-29

### Added

- *(regions)* make reactive settings the default model
- *(regions)* mount keyed route pages through reactive outlets
- *(regions)* expand reactive text styles and adopt settings bindings
- *(regions)* bind text properties to explicit state dependencies
- *(settings)* compare region and immediate preferences APIs
- *(runtime)* restore region state with best-effort checkpoints
- *(runtime)* route host task results to region state IDs
- *(runtime)* own region local state by keyed components
- *(runtime)* bind form controls and keyed lists to native regions
- *(runtime)* add opt-in native regions with tuple callbacks
- *(experiments)* validate ID-driven nested region replay

### Fixed

- *(regions)* align batched events and recover failed replays

### Other

- *(renderer)* reuse paint storage during full-tree flattening
- *(runtime)* skip unrelated adapter work on local updates
- *(runtime)* skip unrelated nodes during geometry alignment
- *(regions)* avoid unchanged animation replays
- *(ui)* share text style setters with reactive bindings

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
