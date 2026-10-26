# Plico

**A GUI framework that respects Rust’s borrow checker.**

[![CI](https://github.com/SunDoge/plico/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/SunDoge/plico/actions/workflows/ci.yml)

English · [简体中文](README.zh-CN.md)

Write UI with ordinary Rust functions, `if`, `for`, and borrowed data. Handle a
click where you declare the button. Plico combines an immediate-mode authoring
API with retained nodes, layout, text, and rendering caches underneath.

**Alpha / 0.1.0 in development.** APIs are evolving; crates are not yet published.

## Why Plico?

- **Work with the borrow checker.** UI declarations and their children closures
  run synchronously. Borrow application data for a declaration, handle events
  inline, and release those borrows when it returns. No mandatory signal wrappers,
  hook ordering, or long-lived event closures capturing your application state.
- **Rust syntax, less visual noise.** `column(cx).gap(12.) + |cx| { ... }` is native
  Rust operator overloading. Use ordinary functions for custom components.
  `#[component]` supplies call-site identity; dynamic lists use business keys.
  `plico fmt` keeps builder chains compact and nested layouts readable.
- **Immediate API, retained work.** Business updates rerun declarations and compare
  properties against retained nodes. Layout, text and paint invalidation are
  tracked separately. Host-driven scrolling and presentation animations can
  advance without rerunning application `show`; idle views need no continuous loop.
- **Local state when you need it.** Keep shared data in ordinary structs, or use
  `cx.state` for component-local state. Named `cx.persisted_state` slots support
  best-effort checkpoint restoration and explicit MessagePack storage export.
- **A short development loop.** Ordinary `main.rs` + `lib.rs` projects, directory-based
  CLI commands, and native module hot reload. The host keeps its window and services
  alive; compatible state is restored, incompatible state can reset. Async work
  stays in host tasks, outside the reloadable UI module.
- **Room for different backends.** Vello is the main desktop path. Optional Skia,
  FLTK + Taffy, LVGL, and browser/WASM paths explore the same declaration model.
  LVGL has been tested on an ESP32-P4 with ESP-IDF.

## A small example

The same function can borrow state owned by its caller. There is no stored event
callback, and the children closure does not need to be `'static`.

```rust
use plico_core::prelude::*;

#[component]
#[rustfmt::skip]
fn counter(cx: &mut UiContext, count: &mut u32) {
    column(cx).gap(12.).padding(24.) + |cx| {
        text(cx, "Counter").font_size(28);
        text!(cx, "Count: {}", count);
        if button(cx).text("Increment").primary().clicked() {
            *count += 1;
        }
    };
}
```

Run the complete [counter example](examples/counter.rs) with `cargo run --example counter`.

Use it inside `plico_dev::App::show`. Application state remains normal Rust data;
components do not need a trait implementation. See [authoring](docs/authoring.md)
and [local state and tasks](docs/local-state-and-tasks.md).

## Try it

From this checkout:

```sh
cargo run --example counter
cargo run -p gallery
cargo run -p launcher
cargo run -p backend-settings

# Generate and develop an ordinary Rust application.
cargo plico new /tmp/my-plico-app
cargo plico dev /tmp/my-plico-app
cargo plico run /tmp/my-plico-app

# Develop the gallery or format UI declarations.
cargo plico dev apps/gallery
cargo plico fmt --check
```

The gallery includes layout and controls, motion, 2048, streaming Markdown, and
in-app backdrop blur. The CLI also generates application-owned SVG icon sets
on demand, so builds do not need an icon registry or all icons bundled together.
See [CLI usage](docs/cli.md), [icons](docs/icons.md), and
[platform dependencies](docs/native-dependencies.md).

## Backends and boundaries

| Path | Current scope |
| --- | --- |
| Vello | Main desktop host, Taffy layout, Parley text, input/IME, SVG and animations |
| Skia | Optional CPU/Vulkan replay and experimental Vulkan window backend; incomplete coverage |
| FLTK + Taffy | Experimental native widgets and shared settings app; limited styling/platform coverage |
| LVGL | Experimental native-widget mapping, SDL2/offscreen validation, ESP32-P4 / ESP-IDF hardware tests |
| Web / WASM | Browser host and split host/UI loading experiments |

Plico is **not** a fine-grained reactive runtime. A business declaration still
executes view code and reconciles nodes; the entire pipeline is not O(changes).
Caching and host-side animation reduce repeated work without changing that model.
Hot reload is best effort, not arbitrary Rust state migration. Backends do not
yet have feature parity; Bevy and `no_std` are not integrated.

## Project map

Framework libraries live in `crates/` with the `plico-` prefix; applications live
in `apps/` without it.

| Area | Packages / directories |
| --- | --- |
| Renderer-independent declarations | `plico-core`: Engine, UiContext, widgets, identity and local state; no window, layout engine or renderer dependency |
| Runtime and desktop host | `plico-runtime`, `plico-vello` |
| Rendering and platform integration | `plico-renderer`, `plico-shell`, `plico-motion`, `plico-native` |
| Alternative backends | `plico-skia`, `plico-fltk`, `plico-lvgl`, `plico-web-host` |
| Development tools | `plico-cli`, `plico-dev`, `plico-macros` |
| Rich content | `plico-markdown`, `plico-editor`, `plico-terminal` |
| Applications | `apps/gallery`, `backend-settings`, `launcher`, `terminal`, `services-demo`, `web-demo`, `lvgl-esp32p4` |

Plico maintains the reused Wabou code here; normal builds do not require a Wabou checkout.

[Documentation](docs/README.md) · [Implementation status](docs/status.md) ·
[Architecture](docs/architecture.md) · [Contributing](docs/contributing.md)
