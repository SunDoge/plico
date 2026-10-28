> Historical experiment: its macros, examples and check scripts have been removed.
> Current authoring uses component + children / optional double semicolons.

# Implicit-context arrow: actual rust-analyzer experiment

> This page records an earlier isolated experiment. The current supported view syntax
> and formatter status are documented in [Single view](../single-view.md).

The proposed syntax removes repeated closure binders and nested macro invocations:

```rust
view_arrow_experiment! {
    column(cx).padding(20) => {
        text(cx, "Settings").font_size(24);
        row(cx).gap(8) => {
            button(cx).text("Save").primary();
            text(cx, "Nested").font_size(14);
        }
    }
};
```

`view_arrow_experiment` is a separate proc-macro in `plico-macros`, used directly
by the `arrow_lsp` example. It is not exported in Plico's prelude and does not change
production `view!` parsing. There is no application migration in this experiment.

## Reproduce

```sh
cargo run -p plico --example arrow_lsp
python3 scripts/experiments/check-arrow-matrix-lsp.py
```

The script speaks actual LSP over rust-analyzer stdio, opens unsaved Rust documents
in an existing Cargo example target, inserts incomplete identifiers and asks for
completion at their end. It also queries context hover and widget definition on
complete documents. Proc-macro expansion is enabled. Detailed generated output is
ignored under `reports/macros/arrow-matrix-lsp.json`.

## Results

Tested server: `rust-analyzer 1.99.0 (b940084 2026-09-28)`.
Each row is tested both with and without an enclosing `#[component]`.

| Syntax | Root header | Nested header | Body builder | Nested button builder | Nested text builder | Context methods | Widget functions |
|---|---|---|---|---|---|---|---|
| Ordinary `.children(\|cx\| {...})` | pass | pass | pass | pass | pass | pass | pass |
| Current `view!` / `=> \|cx\|`, nested `view!` | pass | pass | pass | pass | pass | pass | pass |
| Current `view!` root, ordinary nested `.children` | pass | pass | pass | pass | pass | pass | pass |
| Experimental single macro, recursive `=> {}` | pass | pass | pass | pass | pass | pass | pass |

All **56/56 completion probes** passed after the parser adjustment below.
For each of the eight syntax/component combinations, context hover returned a
UiContext type and button definition resolved into `plico/src/widgets/mod.rs`.
Thus all eight hover probes and eight definition probes also passed. The example
compiled and executed, and existing context-widget/native-children tests passed.

## An actual failure, and its fix

Initially the implicit arrow syntax failed only on the two root-header completion
probes (54/56 overall). Replacing `.padding(20)` with `.padd` leaves a field-access
expression in the temporary completion input. The experimental context inference
handled calls/method calls but rejected this expression, preventing expansion.

Following `Expr::Field.base` when recovering the context fixes both failures.
Token spans are preserved while rewriting groups; the implicit closure binder uses
the original context identifier. The implicit arrow variant then provides typed
member completion, hover and definition navigation in these probes.

This is why complete-file compilation alone is insufficient to evaluate a macro's
editor behavior: its parser must also accept useful incomplete editing states.

## Limits

This is evidence for the listed cases, not full IDE parity. Rename, references,
signature help, diagnostic positioning, latency and arbitrary broken/unbalanced
syntax were not measured. The matrix uses built-in context-first constructors and
one context name, not every custom builder or closure shadowing case.

The experimental token lowerer is deliberately restricted. It recursively rewrites
`=> {}` groups and therefore is **not safe for arbitrary Rust `match` arms**;
a production parser must distinguish UI declarations from normal Rust control
flow. Context inference, expression-valued layouts, macro bodies and identity
semantics need a broader test corpus before adopting this syntax publicly.
Formatting remains a separate problem; this experiment does not implement a
formatter for implicit arrow syntax.
