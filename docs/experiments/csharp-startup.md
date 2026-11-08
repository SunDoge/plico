# C# launch paths

One Linux warm diagnostic, 2026-11-08: the same Controls page was rendered to a PNG
using the existing release native library and a Debug .NET frontend. Hyperfine
ran one warmup and three measurements per command. Background Rust work was
present, so these are indicative development-path costs, not platform guarantees.

| Command | Mean process-to-PNG completion |
| --- | --- |
| `dotnet run --project apps/gallery-csharp -- --page Controls --capture reports/csharp/startup-full.png` | 1.600 s |
| `dotnet run --no-build --project apps/gallery-csharp -- --page Controls --capture reports/csharp/startup-nobuild.png` | 0.806 s |
| `dotnet apps/gallery-csharp/bin/Debug/net10.0/Gallery.dll --page Controls --capture reports/csharp/startup-direct.png` | 0.363 s |

The endpoint includes process/runtime startup, native library and GPU/font
initialization, rendering and PNG writing. It is not window-map/first-present
timing, a cold launch, a hot-reload measurement or a Rust-versus-C# runtime result.
`--no-build` skips project building; direct DLL launch also bypasses the `dotnet
run` project command. Use `dotnet watch` to avoid process/native initialization
when supported edits can be applied in place. C#'s intended gain is UI iteration,
not faster execution of the unchanged Rust renderer.

Reproduce from the repository root with Hyperfine, changing only output paths as
needed. Build the native library and managed gallery once before comparing. Keep
power mode and other workloads stable for a stronger comparison. Generated JSON,
logs and PNGs belong in local reports, not version control.
