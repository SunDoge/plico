> Historical experiment: its macros, examples and check scripts have been removed.
> Current authoring uses component + children / optional double semicolons.

# Call-followed-by-block experiment

> This page records an earlier isolated experiment. The current supported view syntax
> and formatter status are documented in [Single view](../single-view.md).

Question: can `row(cx).gap(8) { ... }` replace an explicit marker, using only an
attribute? Does this syntax break real rust-analyzer completion?

## Results

| Source form | rustc | Completion | Context hover / button definition |
|---|---|---|---|
| Native `.children` | accepts | 6/6 | passes |
| Double semicolon attribute | accepts | 6/6 | passes |
| Bare block attribute | **rejects** | 6/6 | passes |
| Bare block inside single macro | accepts, runs | 6/6 | passes |
| Single macro, context renamed `ui` | accepts shape; LSP probe | 6/6 | passes |
| Single macro, comment before block | accepts shape; LSP probe | 6/6 | passes |

The actual compilation checker directly verifies the first four rows. Renamed and
commented forms are LSP overlays, not separate compilation checks. The complete
matrix passes 36/36 completion probes, six UiContext hover probes and six real
widget-definition probes on rust-analyzer 1.99.0 (b940084 2026-09-28).

**Completion success does not demonstrate compilation success.** In particular,
the bare attribute fixture produces:

```text
error: expected one of `.`, `;`, `?`, `}`, or an operator, found `{`
    column(cx).padding(20) {
```

Rustc diagnoses the invalid function-body syntax despite the attribute's attempted
rewrite. The earlier claim that an attribute can necessarily rescue this syntax
before parsing was incorrect. Ordinary function-body syntax and a macro token-tree
boundary have different constraints.

The negative fixture is deliberately outside Cargo example discovery, under
`scripts/experiments/fixtures/brace_attribute.rs`. It must remain invalid to
characterize this boundary; normal workspace builds should not compile it.

## Compiling form

The example uses a separate experimental proc-macro, not production `view!`:

```rust
view_brace_experiment! {
    column(cx).padding(20) {
        text(cx, "Settings").font_size(24);
        row(cx).gap(8) {
            button(cx).text("Save").primary();
        }
    }
};
```

The experiment recovers the context name from the first layout constructor,
including partly typed method-field expressions such as `.padd`. Context-first
calls followed by blocks are lowered to children closures; existing identity
injection follows. Input tokens/groups retain their spans. The compiling example
also exercises `if`, nested layouts and ordinary `match` arms.

This is a restricted token-lowering prototype, not a general Rust parser. Let-bound
layouts, custom macro contents, complex control-flow expressions, every possible
brace ambiguity and return-value semantics are not covered. No application API or
recommended syntax has changed. There is no dedicated bare-block formatter yet.

## LSP coverage

The six completion locations per variant are root/nested layout headers, body text
builder, nested button builder, context method and widget function. Documents are
opened as unsaved buffers in a real Cargo example target. Hover must contain a
UiContext type; button definition must resolve into `plico/src/widgets/mod.rs`.

These checks do not establish diagnostic quality, references/rename coverage,
signature help, completion latency or error recovery for arbitrary broken code.
The attribute case is intentionally included to demonstrate that successful
completion/navigation can coexist with a hard compiler syntax error.

## Reproduce

```sh
cargo run -p plico --example brace_children
python3 scripts/experiments/check-brace-compilation.py
python3 scripts/experiments/check-brace-children-lsp.py
```

The compiler checker obtains actual library/proc-macro artifacts from Cargo JSON,
then invokes rustc on each fixture in a temporary output directory. It asserts
that only the bare-attribute case is rejected and checks the syntax diagnostic.
It does not overwrite application sources or editor buffers. Detailed reports are
local generated artifacts under `reports/macros/` and are not committed.

The practical choice is therefore:

- No outer macro: keep ordinary children closures or the valid-Rust `;; {}` marker.
- Bare `layout { ... }` syntax: retain one outer macro boundary.
