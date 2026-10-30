# Immediate reconciliation CPU cost

Measured 2026-10-30 on Linux, Intel Core Ultra 9 285K, pinned to CPU 7,
revision `b3765c9`, release optimization with line-table debug info. The
`visible_texts` fixture additionally records main-thread CPU time around its
entire measured batch (two CLOCK_THREAD_CPUTIME_ID reads, not per node).
No GPU rendering, rasterization, window presentation or tracing is included.

## What counts as diff

Plico does not have a single standalone VDOM diff pass. Comparisons are spread
across declaration and backend synchronization. This experiment distinguishes:

- Node reconciliation: Engine::declare, set_grid and set_spans. This includes
  stable-ID matching, text/style comparison, bookkeeping and actual node writes;
  it is broader than just equality operators.
- Commit reconciliation: the post-view part of Engine::frame_inner that checks
  child order, removes absent nodes and commits the dirty journal.
- Adapter synchronization: Adapter::sync and the pre-layout portion of
  Adapter::update_retained. This includes dirty-node bookkeeping and style/paint
  updates; it is not a pure property-comparison benchmark.

Application declaration/builders, Taffy layout, flatten, Scene encoding,
SkParagraph and rendering are not classified as diff. They remain in the total
CPU-preparation denominator.

## Method

Build the optimized fixture with symbols:

```sh
CARGO_PROFILE_RELEASE_DEBUG=1 cargo build --release -p backend-settings \
  --features skia --example visible_texts
```

CPU durations were collected without a debugger, in three rounds, alternating
workload order. Each run used 500 warmup + 3000 measured updates:

```sh
taskset -c 7 target/release/examples/visible_texts \
  --headless --immediate --warmup 500 --updates 3000
```

Add `--downloads` for the task dashboard; add `--unchanged` to force a declaration
without changing its inputs. This unchanged control is deliberately not an idle
window: an idle event-driven window should not redeclare at 60 Hz.

Separately, GDB/MI async sampling launched the same binary with 1,000,000
updates, let it run for two seconds, then interrupted it at seeded random
1–6 ms run intervals. Each of the four workloads received 1200 main-thread
stack samples; the sparse-text and sparse-dashboard cases were repeated. GDB
was the parent of the controlled process, so no system ptrace settings were
changed. No instruction stepping or breakpoints were used during sampling.

Stacks (including inlined frames) identify node reconciliation by the three
Engine functions above. Commit samples use engine.rs lines 896–1034 of the
measured revision; sync samples use Adapter::sync or the update_retained caller
before its sync timer ends at lib.rs line 618. The subsets at Engine::declare
lines 621 and 662 identify style equality and the text equality branch.
These line-based classifications must be re-audited when source code changes.

Estimated stage CPU time = sampled fraction × median batch CPU time/update.
The debugger's paused time is never used as application CPU duration. This is
statistical attribution, not exact timer instrumentation: small stages can be
missed, optimized source lines can be imprecise, and short samples do not
establish narrow confidence bounds. Repeated runs corroborate the scale, not
nanosecond precision.

## Results

| Workload | Total preparation CPU/update | Reconciliation estimate | Share of preparation | Equivalent one-core CPU at 60 updates/s |
|---|---:|---:|---:|---:|
| 500 visible texts, one changes | 310 µs | about 35 µs | about 11–12% | about 0.21% |
| 48 transfer rows, one progress changes | 925 µs | about 65–70 µs | about 7% | about 0.4% |
| Same text grid, forced unchanged declaration | 81 µs | about 33 µs | about 41% | about 0.20% |
| Same dashboard, forced unchanged declaration | 184 µs | about 69 µs | about 38% | about 0.42% |

Text-grid declarations visit 1027 nodes; the dashboard visits 2116. Whole
application declaration costs roughly 77–79 µs and 182–187 µs respectively;
calling all of that diff would overstate reconciliation cost.

Style equality alone was approximately 14–15 µs for the text grid and
approximately 28 µs for the dashboard. These are sampled subsets, already
included in reconciliation above, not additional costs. Backend sync was below
1% of total preparation in these samples; low sample counts prevent a precise
small-cost estimate.

At 144 updates/s the same 35/69 µs would contribute approximately 0.50%/0.99%
of one core. These are frequency conversions, not measured native-window CPU
percentages, and cannot be extrapolated to ESP32 cycle costs.

The unchanged controls show why reconciliation can dominate a cheaper frame
without becoming more expensive in absolute terms. The measured workloads do
not support blaming their much larger native-window process CPU consumption
primarily on diff. Re-executing builders and the shared backend/rendering path
are separate costs.

Generated stacks and CSVs remain local in ignored reports/. The fixture itself
is checked in, and its `mean_thread_cpu_us` column is NA on platforms without
the Linux thread CPU clock. Without --immediate, headless mode still runs both
models and asserts output parity; --immediate selects only the immediate path
for profiling.
