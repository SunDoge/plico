# Widget builder lifetimes: keep the direct borrow

The current representation is deliberate:

```rust,ignore
pub fn surface<'u, 'e>(cx: &'u mut UiContext<'e>) -> Layout<'u, 'e> {
    column(cx).padding(16).gap(8)
}
```

`'u` is the builder's exclusive borrow of the declaration context. `'e` is the
context's borrow of the engine. Text/control builders can also have a third
lifetime for the borrowed label or controlled value. These are different borrows;
forcing them to be equal can keep the context borrowed for too long and prevent
later sibling declarations.

Ordinary components do not need these parameters:

```rust,ignore
#[component]
fn heading(cx: &mut UiContext, label: &str) {
    text(cx, label).font_size(24).weight(600);
}
```

Only a function or struct that returns/stores a configurable borrowed builder
usually needs explicit lifetime names. Prefer an ordinary component when no
further configuration is required. Do not allocate a label just to hide its lifetime.

## Rejected short-context experiment (2026-10-24)

A local experiment changed builders from `&mut UiContext<'engine>` to an owned,
short reborrow of the engine plus declaration metadata. This removed the engine
lifetime from their public type. Only inherited font size/weight/color/disabled
flags were carried, rather than the entire style. There was no new heap allocation,
reference counting, dynamic dispatch or unsafe lifetime erasure.

Nevertheless, the new runtime representation regressed declaration performance.
Inlining constructors/helpers reduced but did not eliminate the regression. The
change was reverted; it is not an implemented API or an optimization proposal.

Measured against core code at commit `69ebad1`, release profile, identical existing
probes, pinned to one CPU, four alternating before/after process pairs. Values are
medians of the probes' per-process p50 frame times, in microseconds, after inlining:

| Probe | Direct borrow | Short context | Change |
|---|---:|---:|---:|
| Stable borrowed labels, 1,000 rows / 3,000 nodes | 204.04 | 221.67 | +8.6% |
| Sparse borrowed labels, 1,000 rows / 3,000 nodes | 204.89 | 220.05 | +7.4% |
| Stable borrowed labels, 10,000 rows / 30,000 nodes | 2374.19 | 2550.48 | +7.4% |
| Sparse borrowed labels, 10,000 rows / 30,000 nodes | 2342.85 | 2589.89 | +10.5% |
| All styles changing, 3,000 nodes | 352.71 | 402.50 | +14.1% |
| Reordering, 3,000 nodes | 262.63 | 270.80 | +3.1% |

Before explicit inlining, the same workload suite regressed about 8–19%.
These numbers are local CPU measurements, not cross-platform performance promises
or rendering benchmarks. They establish a rejection gate for this implementation;
they do not prove that every possible one-lifetime design must be slower. We did
not isolate the exact code-generation cause. Allocation counts alone would have
missed this regression.

The reusable probes remain in the repository:

```sh
cargo run --release -p plico --example memo_probe
cargo run --release -p plico --example declaration_churn
cargo run --release -p plico --example allocations
```

Future simplifications should preserve the direct-borrow representation, or show
no measurable regression across representative workloads before replacing it.
Compile-time syntax conveniences may be explored separately, but must also preserve
ordinary Rust borrowing and useful language-server diagnostics. Do not add unsafe
type erasure or a second ownership model solely to remove lifetime annotations.
