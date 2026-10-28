# Double-semicolon children experiment

> 历史实验记录：双分号语法及旧 Python formatter 已移除，当前写法与验证见 [operator children](operator-children.md)。

> This page records an earlier isolated experiment. The current supported view syntax
> and formatter status are documented in [Single view](../single-view.md).

This implementation is now available through production `#[component]` as optional
syntax sugar. Native `.children(...)` remains supported; the experimental attribute
is a compatibility alias.
The source is valid Rust syntax before attribute expansion:

```rust
#[component]
#[rustfmt::skip]
fn settings(cx: &mut UiContext, extra: bool) {
    column(cx).padding(20);; {
        text(cx, "Settings").font_size(24);
        row(cx).gap(8);; {
            button(cx).text("Save").primary();
        }
        match extra {
            true => { text(cx, "Expanded"); }
            false => { text(cx, "Compact"); }
        }
    }
}
```

## Expansion and runtime checks

Syn parses a marked layout as three statements: an expression with a semicolon,
an empty statement, and an unlabeled block. The experimental attribute folds the
triple into `layout.children(|cx| { ... });`, recursively, then invokes the
existing component identity transformation. It uses the context identifier from
the function signature, not a hard-coded `cx` name. Local function items are not
rewritten by their containing function's pass.

`tests/semi_children.rs` verifies:

- Nested parent/child relationships, not just successful compilation.
- Stable button and nested text identities when a preceding conditional node is
  removed, including delivery of a queued click exactly once.
- A function whose context is called `ui` works.
- Ordinary `logic(value); { ... }` stays ordinary Rust and executes once per frame.
- Normal `match` arms and comments between the two semicolons work.

The marked block has closure semantics after expansion: `return`, `?`, outer-loop
`break` and captures must be understood accordingly. Being parseable Rust does
not mean the unexpanded and expanded programs have the same semantics.

## Actual rust-analyzer results

On `rust-analyzer 1.99.0 (b940084 2026-09-28)`, six incomplete-input completion
probes passed for all five source variants:

| Source variant | Completion | Context hover | Button definition |
|---|---|---|---|
| Native children baseline | 6/6 | UiContext | real widget source |
| Double-semicolon attribute | 6/6 | UiContext | real widget source |
| Function context renamed to `ui` | 6/6 | UiContext | real widget source |
| Comment between marker semicolons | 6/6 | UiContext | real widget source |
| Only one semicolon typed so far | 6/6 | UiContext | real widget source |

The six locations are root layout header, nested layout header, text builder,
nested button builder, context method and widget function. That is 30/30
completions, five hover and five definition probes. In the single-semicolon
editing state, the function still parses and LSP works, but the block is NOT a
children block until the second semicolon is inserted.

The subsequent [fresh-call experiment](fresh-call-lsp.md) tested eight editing
states at both root and nested positions, including an unclosed `text(`.
Returning the original function plus a compile diagnostic when attribute parsing
fails restored completion and text signature help in all 16 cases. Invalid
source still fails compilation. This is a narrower and simpler recovery than
reconstructing statements inside the bare-block view DSL.

Rename/reference coverage, general diagnostic quality, completion latency and
arbitrary closure shadowing were not measured.

## Formatting strategy

The component formatter recognizes `#[component]` paired with explicit
`#[rustfmt::skip]`.
Before passing a temporary copy to rustfmt, it protects each marker's empty
statement using a reserved placeholder macro statement. After rustfmt it restores
the empty statement and joins whitespace to emit `;; {` when comments allow.
The existing method-chain pass then flattens short layout headers.

The formatter writes stdout only and splices only marked function ranges. Tests
cover nested markers, inline/block comments, raw strings containing `;; {`, normal
match arms, untouched outside bytes, idempotence at 60/80/120 characters and
composition with ordinary rustfmt. No reliance is placed on rustfmt preserving
redundant semicolons.

This remains a prototype, not an editor-integrated formatter. Whole-function Rust
formatting is delegated to rustfmt on the protected copy; unrelated method chains
inside components also receive the chain rule. The existing formatter limitations
in [the native-children experiment](component-format.md) still apply.

## Reproduce

```sh
cargo run -p plico --example semi_children
cargo test -p plico --test semi_children
python3 scripts/experiments/check-semi-children-lsp.py
uv run scripts/experiments/check-component-format.py
uv run scripts/experiments/format-components.py crates/plico/examples/semi_children.rs
```

The formatter's Python dependencies are pinned in script metadata and isolated by
uv. LSP reports under `reports/macros/` are generated local artifacts, not committed.
