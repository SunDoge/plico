# Visible text sparse-update comparison

These are historical baseline measurements taken before the incremental backend
work in `564074b` and its follow-up fixes. The example uses the current runtime;
rerunning it does not reproduce the old backend implementation. Re-measure both
models on the same revision before drawing conclusions about current performance.
The current CSV reports both top-level command slots and expanded command counts:
a retained Fragment is one slot, not one draw call. The reactive fixture forwards
its Scene cache roots and asserts that it actually exercises fragment encoding.

Measured on Linux / Intel Core Ultra 9 285K (24 logical CPUs), release build,
Skia backend, 2026-10-29. The example uses a shared 25-column, 20-row layout:
500 distinct visible labels, with only the first label changing. Both paths
produce exactly equal final text and geometry; headless assertions also verify
that all 500 labels fit in the viewport. This is a property-reactive comparison,
not a structural region-rebuild benchmark.

```sh
cargo run --release -p backend-settings --features skia --example visible_texts
cargo run --release -p backend-settings --features skia --example visible_texts -- --immediate
cargo run --release -p backend-settings --features skia --example visible_texts -- --headless --warmup 100 --updates 300
```

## CPU frame preparation

Three runs pinned to one CPU; table shows the median of each run's reported
statistic. Timings include state mutation, declaration/replay, backend update,
and CPU Scene encoding with SkParagraph. They exclude rasterization and GPU
submission. Initial construction and 100 warmup updates are excluded.

| Metric | Immediate | Property-reactive |
|---|---:|---:|
| Total p50 | 387.53 µs | 289.68 µs |
| Total p95 | 451.41 µs | 349.65 µs |
| Mean declaration | 86.87 µs | 0.09 µs |
| Declaration nodes visited | 1027 | 0 |
| Flatten visits | 1028 | 1028 |
| Layout runs | 1 | 1 |
| Scene commands | 504 | 504 |

Reactive reduces CPU preparation by approximately 25% here, primarily by
avoiding declaration. Both paths still traverse the shared backend.

## Real window

1440 × 1060 native Wayland windows, Skia, requested update frequency 60 Hz.
Four sequential runs in immediate/reactive/reactive/immediate order, each with
3 seconds settling and 6 seconds sampling. Process CPU includes all threads.
Immediate consumed 21.0% and 17.5% of one core; reactive consumed 18.8% and
17.3%. This is a noisy, small sample, insufficient to claim a reliable window
CPU speedup. Completed presentation rates were not measured. On this machine,
18% of one core corresponds to 0.75% of total 24-core capacity.

## Cache capacity matters

The earlier 1000-label fixture took roughly 6.62 ms immediate / 6.41 ms
reactive for CPU preparation and approximately 75% of one core in native
windows. SkParagraph's 512-entry cache thrashed. Reducing the fixture to 500
labels removed the sustained large-scale churn without changing the renderer
or cache policy.

A separate profiling-enabled probe (2 warmup + 10 measured updates, with
untimed raster draws before and after) recorded paragraph miss/eviction deltas
of 48/48 on both 500-label paths, versus 13075/13075 with 1000 labels. These
deltas include the final raster replay and are not per-update measurements.
The 500-label case still has some eviction; it does not imply a zero-miss cache.

These results do not establish a Rust-vs-Go or Plico-vs-MyGO advantage:
renderer, fonts, scene details and achieved presentation rates must also match.
Generated CSVs and traces remain local under ignored reports/.

## Damage tracking enabled

A separate interleaved comparison explicitly set `PLICO_SKIA_DAMAGE=0/1`;
other experimental present/multi-rect switches were disabled. Each condition
had two runs, 2 seconds settling and 4 seconds sampling, without tracing.
Mean process CPU as a percentage of one core:

| Mode | Damage off | Damage on |
|---|---:|---:|
| Immediate | 19.50% | 14.62% |
| Property-reactive | 19.75% | 14.12% |

This short sample suggests approximately 25–28% lower process CPU. It is not a
controlled achieved-frame-rate comparison. A separate 3-second trace confirmed
steady-state target damage coverage around 0.030–0.031%, while
`present_damage_coverage` remained 100%: local Skia drawing was enabled, but
incremental presentation was not. Damage tracking does not eliminate full
Scene preparation or command comparison.

```sh
PLICO_SKIA_DAMAGE=1 cargo run --release -p backend-settings --features skia --example visible_texts
```

The example honors `RUST_LOG` for optional diagnostics. Do not enable trace
logging during CPU comparisons.

## Native phase breakdown with damage

A diagnostic build enables selected existing phase spans. The aggregate
collector ignores per-node spans, skips the first 2 seconds, and prints once
after 300 measured frames. On Linux, each span records both wall duration and
`CLOCK_THREAD_CPUTIME_ID` duration, so fence/acquire waits are not confused with
CPU consumption. Nested spans overlap: do not sum parent and child rows.

```sh
PLICO_PHASE_STATS=1 PLICO_SKIA_DAMAGE=1 cargo run --release \
  -p backend-settings --features skia,profiling,plico-shell/profiling \
  --example visible_texts
```

Representative real-window run, mean main-thread CPU duration per frame:

| Phase | Immediate | Property-reactive |
|---|---:|---:|
| Entire redraw | 2.571 ms | 2.239 ms |
| Retained frame preparation (includes declaration and Scene encoding) | 1.792 ms | 1.452 ms |
| Declaration (included above) | 0.313 ms | no full declarations in sample |
| Layout phase (included in preparation) | 0.400 ms | 0.396 ms |
| Flatten (included in preparation) | 0.294 ms | 0.284 ms |
| Backend presentation processing (included in preparation) | 0.126 ms | 0.121 ms |
| Renderer present (includes the following renderer phases) | 0.758 ms | 0.767 ms |
| Damage comparison | 0.097 ms | 0.098 ms |
| Skia local draw | 0.069 ms | 0.067 ms |
| Skia flush/submit | 0.448 ms | 0.462 ms |
| Vulkan queue present | 0.046 ms | 0.045 ms |
| Fence wait | 0.013 ms | 0.012 ms |

The host's existing wall timers additionally measured preparation before Scene
encoding at 1.283 / 0.954 ms and Scene encoding at 0.506 / 0.497 ms. These two
timers do not independently record thread CPU time; the collector labels their
CPU column NA. Their containing span has nearly equal wall and thread CPU
measurements in this sample.

These diagnostic native timings must not be substituted for the earlier
CPU-pinned headless baseline: execution environment, scheduling/frequency and
instrumentation differ. They characterize this native pipeline, not an exact
prediction of optimization speedups. A separate Chrome trace observed roughly
59–60 redraws/s. Whole-process CPU also includes background/driver threads;
main-thread span CPU is not the complete process total or GPU execution time.

The useful priority is reducing shared frame-preparation work: full flatten,
backend processing and Scene encoding remain significant even when reactive
skips declaration. Damage already makes drawing cheap. Flush/submit is another
measurable CPU cost, but this measurement alone does not distinguish driver
work from Skia encoding or establish how much can be eliminated. Broad Chrome
tracing is available through PLICO_TRACE_FILE with plico-shell/profiling;
per-node tracing can perturb results and should remain a separate diagnostic.

## Batch-download dashboard workload

`--downloads` selects a realistic dashboard-shaped workload: 48 simultaneously
visible transfers, each with filename, status, size, a 32-segment progress
meter, percentage, and two buttons. One transfer's progress changes at the
requested display frequency; others are paused/completed. Data is simulated:
there are no network downloads, and the buttons are visual fixtures, not a
complete download manager. This tests a continuously updating dashboard, not
the CPU consumption of a normally throttled background downloader.

The two paths share the same builder and cache immutable labels. There is no
sorting, parsing, artificial sleep, or expensive calculation added to show.
Immediate redeclares the dashboard; reactive replays the active meter's small
watch region and updates its percentage binding. No immediate memo boundary
is used, so these measurements do not establish an advantage over an
explicitly invalidated/cached immediate component.

```sh
PLICO_SKIA_DAMAGE=1 cargo run --release -p backend-settings --features skia \
  --example visible_texts -- --downloads
PLICO_SKIA_DAMAGE=1 cargo run --release -p backend-settings --features skia \
  --example visible_texts -- --downloads --immediate
cargo run --release -p backend-settings --features skia --example visible_texts \
  -- --downloads --headless --warmup 100 --updates 299
```

Three pinned headless runs (median of each reported statistic):

| Metric | Immediate | Reactive |
|---|---:|---:|
| Total CPU preparation p50 | 1074.64 µs | 900.20 µs |
| Mean declaration | 189.32 µs | 7.17 µs |
| Declaration nodes visited | 2116 | 33 |
| Flatten visits | 2117 | 2117 |
| Scene commands | 1923 | 1923 |

CPU preparation is approximately 16% faster. The final snapshots compare all
placed-node text, background, opacity and geometry; all nodes must be inside
the viewport. Additional 1/32/50/99-update parity runs cover different meter
states; final progress is asserted against the expected state. This is not a
pixel-level or complete behavioral equivalence test.

Six interleaved native-window runs, three per model, with damage enabled,
2 seconds settling and 4 seconds sampling, no tracing/HUD:

| Model | One-core process CPU samples | Mean |
|---|---|---:|
| Immediate | 26.50%, 25.75%, 25.25% | 25.83% |
| Reactive | 23.25%, 23.00%, 23.00% | 23.08% |

The observed reduction is 2.75 percentage points of one core, approximately
11% relative. A separate frame-trace diagnostic of this workload observed
59.82 versus 59.32 completed redraws/s, so no large frame-rate reduction was
seen. This is a short local comparison, not a universal framework result.
Both paths retain substantial shared backend work; this realistic workload
demonstrates a repeatable modest saving, not an order-of-magnitude advantage.
