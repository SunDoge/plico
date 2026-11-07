# Frontend / native-boundary benchmark

From the repository root:

```sh
cargo build --locked -p plico-csharp
dotnet build -c Release crates/plico-csharp/dotnet/InteropBench
PLICO_NATIVE_LIBRARY="$PWD/target/debug/libplico_csharp.so" DOTNET_TieredCompilation=0 \
  dotnet crates/plico-csharp/dotnet/InteropBench/bin/Release/net10.0/Plico.InteropBench.dll \
  64 reports/csharp/interop.json
```

Use the appropriate native library path on macOS/Windows. On Linux, optionally
prefix `dotnet` with `taskset -c <allowed CPU>` for more consistent placement.
Keep each measured binary/dependency directory immutable during a run. Compare
variants sequentially on the same native library and build profiles.

The first positional argument is samples (16–10000), the second is output JSON.
The harness first checks malformed frame headers, UTF-8 spans/content, duplicate
IDs and valid activation. It then measures:

- Isolated C# declaration/encoding, with native calls excluded.
- The actual owned-copy/validation/drop path over FFI, without Engine/layout/text.
- CPU `Step`: callbacks, adoption, declarations, Engine diff, text/layout and scene
  preparation, without a window or GPU. Native phase counters are microsecond
  quantized and reported as aggregate means; they are not raw FFI latency.
- Pointer down/up → native activation → managed handler → settled CPU frame.
  This last case increments nonvisual state behind one stable button.

Cases use 100/1000/10000 text nodes, fixed business scopes, precomputed equal-length
strings and idle/same/sparse/all changes. Buttons and closed selects use 100/1000
widgets; select options are preconstructed. There is no virtualization. The
10,000-label all-changing CPU frame is explicitly omitted to avoid benchmarking
bulk system text layout under the guise of transport cost; its managed-build and
copy costs are still measured. The 1000-label all-changing CPU case is included.

Managed declaration warmup is 128 iterations; native/copy warmup is 16.
Allocations are measured on the managed UI thread after warmup. This does not
count Rust allocation, object initialization, native text memory, or application
code allocating its own strings/options. Payload bytes are encoded size, not
steady-state heap residency. Forced idle `Step` calls are not sleeping-app CPU.

Results are generated under ignored `reports/`. See the repository's
[experiment notes](../../../../docs/experiments/csharp-interop.md) for observed
results and interpretation limits. No framework or application source is edited
by this benchmark.
