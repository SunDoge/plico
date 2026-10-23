# Native children / skipped component formatting experiment

> 历史实验记录：双分号语法及旧 Python formatter 已移除，当前写法与验证见 [operator children](operator-children.md)。

> This page records the native-children formatter experiment. Current authoring
> supports both native children and component `;; {}`; view macros have been removed.
> See [Authoring](../authoring.md).

## Reproduce

```sh
cargo run -p plico-core --example native_children
cargo test -p plico-core --test native_children
uv run scripts/experiments/check-component-format.py
uv run scripts/experiments/format-components.py crates/plico-core/examples/native_children.rs
python3 scripts/experiments/check-native-children-lsp.py
```

The formatter prints to stdout and never overwrites input. `uv` installs pinned
Tree-sitter Python bindings in an isolated script environment. Rustfmt and
rust-analyzer must be on PATH. LSP JSON reports are ignored local artifacts under
`reports/`; they are not needed to understand these conclusions.

## Observed results

The example compiles and runs without `view!` or `=>`:

```rust
#[component]
#[rustfmt::skip]
fn settings(cx: &mut UiContext, expanded: bool) {
    column(cx).gap(16).padding(20).children(|cx| {
        text(cx, "Settings").font_size(24).weight(650);
        if expanded {
            row(cx).gap(8).children(|cx| {
                button(cx).text("Optional");
            });
        }
        row(cx).gap(8).children(|cx| {
            button(cx).text("Save").primary();
            button(cx).text("Cancel").ghost();
        });
    });
}
```

- With `cx` reused as the closure parameter, conditional insertion/removal retains
  the following button's entity and queued click, including nested layout identity.
- All five actual rust-analyzer completion probes passed: layout header, children
  method, nested button builder, context method inside a closure and widget function.
  This is completion evidence, not exhaustive rename/navigation/diagnostics coverage.
- Explicit `#[rustfmt::skip]` preserves the function through ordinary rustfmt.
- Formatter output is idempotent at tested widths 60/80/120. Running rustfmt then
  the component formatter does not alternate layouts in the fixture.
- Tested comments, a raw string containing code-like text and every byte outside
  selected functions survive. Invalid syntax is rejected.

## Prototype strategy and limits

Tree-sitter identifies functions with both literal `#[component]` and
`#[rustfmt::skip]`. A temporary copy replaces only their skip attributes with a
private marker and passes through rustfmt. Formatted function spans are extracted,
then whitespace before method-chain dots is joined while the line fits (120
characters by default). Token leaves are compared before/after joining. Only
function spans are spliced into the original source; outside bytes are untouched.
Trailing closures remain ordinary Rust.

This delegates ordinary Rust indentation and expression formatting to rustfmt,
while the prototype owns chain wrapping. It does not yet distinguish UI chains
from unrelated method chains within selected functions. It is not a complete
formatter: nested marked functions are rejected, namespaced component attributes
are not recognized, display-cell width is not calculated, and long arguments use
rustfmt's ordinary wrapping. File checking, atomic writes, CLI/editor integration
and a broader syntax corpus remain future work.

The original experiment found a limitation when renaming a children context from
`cx` to another identifier. This has since been fixed: component tracks the actual
children closure binding, and native_children tests now require stable identity
across conditional siblings with that renamed context. This does not imply type
analysis of arbitrary unrelated closures or custom method signatures.
