# C# / Rust interaction measurements

Outgoing batches also fail before growing beyond the native node/UTF-8 budgets.

The C# SDK package ID and assembly name are `SunDoge.Plico`; the namespace remains
`Plico`. This is package metadata, not a claim that native runtime packages have
been published to NuGet.

## Method

The reproducible harness is `crates/plico-csharp/dotnet/InteropBench`. It measures
managed construction, actual native owned-copy/validation/drop, and CPU frame
preparation separately. There is no window or GPU. It also validates malformed
incoming event frames and checks that each injected pointer activation is
consumed exactly once. Frame validation cannot make arbitrary foreign pointers
safe: native buffers must still be readable and alive for the call.

Observed locally on Intel Core Ultra 9 285K / Debian 13 x64 / .NET 10.0.14, pinned
to CPU 0. C# Release, tiered compilation disabled; native `cargo build` uses the
workspace dev profile (opt-level 1, registry dependencies opt-level 2). There are
64 samples after 128 managed / 16 native warmup iterations. This is not a
cross-machine performance guarantee. Frontend variants ran sequentially from
separate immutable output directories against the same native library.

Fixtures precompute strings and option arrays. All nodes are declared; only a
subset of rows is visible and there is no list virtualization. Sparse changes
modify one label, all changes modify every label. Idle forces native CPU Steps
but must produce zero managed callbacks. CPU frame preparation includes Engine,
text/layout and scene work; it is not just serialization or property comparison.

## Frontend optimizations

The baseline keeps the new frame validation but retains repeated button UTF-8
encoding, LINQ state-table pruning and closed-select validation/capture allocation.
The optimized path shares button label spans, reuses pruning/validation scratch
storage and moves the capturing search predicate into the open-only select path.

| Warm C# construction, p50 | Baseline | Optimized | Managed temporary allocation/update |
| --- | ---: | ---: | ---: |
| 1000 text nodes, forced same declaration | 174 µs | 177 µs | 240 B → 0 B |
| 1000 buttons | 302 µs | 209 µs | 240 B → 0 B |
| 1000 closed selects, 3 prepared options each | 699 µs | 374 µs | 384280 B → 0 B |

The text timings do not show a speed improvement; small timing changes are not
attributed to the optimization. Allocation removal is the reliable result there.
Buttons improve by about 31% in managed construction, closed selects by about
47% in this fixture. These are not whole-application gains. Button UTF-8 payload
halves from 26000 to 13000 bytes, but total batch size falls only from 330152 to
317152 bytes because node records dominate.

Zero allocation refers to these warmed managed fixture paths. Rust still copies
and allocates owned data. Initial widget state, changed text, open popup filtering,
async messages and application-created arrays/strings can allocate.

## Optimized transport and CPU frame

| Fixture | Managed build p50 | Copy + validation + drop p50 | CPU Step p50 |
| --- | ---: | ---: | ---: |
| 1000 text, forced same | 177 µs | 34 µs | 421 µs |
| 1000 text, one change | 177 µs | 34 µs | 666 µs |
| 1000 text, all change | 178 µs | 35 µs | 44.0 ms |
| 10000 text, forced same | 1.85 ms | 366 µs | 3.89 ms |
| 10000 text, one change | 1.87 ms | 370 µs | 7.28 ms |
| 1000 buttons | 209 µs | 87 µs | 741 µs |
| 1000 closed selects | 374 µs | 87 µs | 905 µs |

A batch command occupies 152 bytes. A 1000-text fixture sends 165152 bytes; the
10000-text fixture sends 1650152 bytes. The direct copy test is frozen encoded
data: it measures the boundary independently of shaping, not a second UI model.
Phase p50 values are independent and must not be added to claim a frame p50.
Native in-frame counters are aggregate means and include adoption work, unlike
the isolated copy/drop probe.

One stable-button pointer round trip measured about 5.6 µs p50 and 0 B managed
temporary allocation after warmup. It includes two automation P/Invokes plus a
CPU Step and settling; the changed counter is nonvisual. This is not raw P/Invoke
latency or a typical click that changes text and layout.

The 1000-label all-changing case is much slower than managed construction and
transport combined. This identifies downstream CPU work as the next profiling
target; it does not prove whether shaping, layout or scene preparation dominates.
The 10000-label all-changing case measures build/copy only and explicitly omits
its expensive CPU frame; there is no estimated frame number for that case.

## Implications

- Preserve the owned-copy lifetime boundary; the current data does not justify
  keeping unmanaged pointers into mutable managed buffers to save copy time.
- C# widget implementation can have allocation-free warm paths without signals
  or per-widget native callbacks. That requires avoiding incidental captured
  closures/LINQ and reusing batch/identity storage.
- At large N, full managed declaration plus retained-tree work still matters.
  This architecture does not remove that cost automatically.
- The next transport experiment should examine compact node/style tables or
  repeated-property reuse: current fixed node records dominate short-label data.
  Full-text updates need separate native profiling before changing the bridge.
