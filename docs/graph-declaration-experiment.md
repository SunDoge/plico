# Workflow graph declaration workload

This synthetic workflow graph retains 512 cards with 12 ports each (30,214 UI
nodes). Only the first card's port colors change on each tick. Labels repeat so
this does not manufacture text shaping cache churn. There are no network tasks,
edge drawing, graph editing, or expensive business computations.

Run a real Skia window (default: local region replay):

```sh
cargo run --release -p backend-settings --features skia --example graph_declaration
# Full immediate declarations:
cargo run --release -p backend-settings --features skia --example graph_declaration -- --immediate
# Immediate with declaration-time viewport culling:
cargo run --release -p backend-settings --features skia --example graph_declaration -- --immediate --visible-only
```

The camera/culling rectangle is fixed at 1440 × 920 logical pixels. This is a
workload, not a complete graph editor; resize does not change that rectangle.

For isolated state mutation + declaration + reconciliation, without layout,
text shaping, scene encoding, rasterization, GPU, or a window:

```sh
cargo run --release -p backend-settings --features skia --example graph_declaration -- --headless --nodes 512 --updates 300
```

All four paths (including regions with the same viewport culling) compare the
final visible node kinds, styles and text. This is declaration parity, not a
pixel/geometry assertion. Headless measured loops do
not initialize a renderer. Window CPU includes shared downstream work and must
not be interpreted as declaration CPU.

## Local result

2026-10-30, Intel Core Ultra 9 285K, Linux, release, thread pinned to CPU 7,
40 warmup + 300 measured updates. One run; not a cross-machine guarantee.
Mean thread CPU microseconds per update (p50 wall time is similar):

| Graph cards | Full immediate | Culled immediate | Local region |
| --- | ---: | ---: | ---: |
| 64 | 330.0 | 61.2 | 8.8 |
| 512 | 2869.3 | 117.1 | 8.5 |

At 512 cards, declarations visited 30,214 / 1,422 / 57 nodes respectively.
At 60 updates/sec full declarations consume about 17.2% of one core in this
stage, versus 0.70% with culling and 0.05% with regions. These are arithmetic
CPU equivalents, not observed whole-process CPU utilization.

The meaningful conclusion is twofold: retaining a large offscreen graph while
executing every card can cost milliseconds, but viewport culling already removes
most of this cost. Regions save a further ~109 microseconds here, not 2.86 ms
against an appropriately culled immediate implementation. This does not prove
frame drops, nor identify cache misses as the cause.

## Native window CPU follow-up

Skia window on the local XWayland display, release, requested tick frequency
60 Hz; process CPU from `/proc/PID/stat`, including all process threads. One
fully occupied core = 100%. The first round used 4 seconds startup plus three
5-second samples; the reverse-order round used 3 seconds startup plus two
4-second samples. Other applications/compositor were not isolated.

| Path | First-round mean | Reverse-order mean |
| --- | ---: | ---: |
| Full immediate | 81.86% | 82.74% |
| Culled immediate | 67.00% | 59.87% |
| Local region | 72.26% | 63.99% |

These short whole-window samples have substantial variance. They demonstrate
high total CPU in this workload, but **do not establish a region advantage over
culled immediate**. They include downstream layout/text/scene/render/driver work;
the isolated declaration figures above cannot be substituted for total CPU.
Presented frame counts/GPU duration were not measured, so this is not an
FPS-normalized renderer comparison. Host compositor CPU is excluded.

An earlier Wayland attempt processed state ticks but declared only the initial
frame; its low CPU readings were rejected. Temporary diagnostics confirmed
ongoing declarations on XWayland before collecting the samples above.

A follow-up gave regions the same declaration-time viewport culling:
57.19%, 69.19%, 69.19% process CPU (mean 65.19%). The original region-vs-culled
comparison had different retained tree sizes (30,214 vs 1,422), so it must not
be interpreted as a controlled comparison of the two APIs. Even with matching
culling these short runs do not resolve a reliable API advantage. The isolated
117.1−8.5 µs saving corresponds to only ~0.65 percentage points of one core at
60 updates/sec, much smaller than the observed whole-window variation. These early measurements did not identify which downstream stage dominates.
Subsequent frame-counted tracing is recorded in
[the performance report](performance.md#稀疏-graph-的真实-xwayland-窗口-profiling2026-10-30).
The window figures above predate default damage tracking and must not be presented
as current default-window CPU. On platforms without the Linux thread CPU clock,
the headless CPU column reports `NA` rather than a misleading zero.
