# C# launch paths

> 历史实验：C# 前端与跨语言 ABI 已从当前源码树移除。本文保留测量结论；其中命令和源码路径仅适用于对应历史提交。

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

## Incremental build diagnostic

Measured separately on 2026-11-08, Linux x86-64, Intel Core Ultra 9 285K,
Rust 1.99.1 and .NET SDK 10.0.403. These are wall-clock build times, excluding
application startup, GPU initialization and rendering. No clean builds were run.
Rust used the normal local `cargo`/mbx wrapper, `dev` opt-level 1, dependency
opt-level 2, debug information and incremental compilation. Logs showed zero
mbx compilation hits for the edited builds; incremental compiler work was used.

Commands:

```sh
cargo build --locked -p gallery --bin gallery
dotnet build apps/gallery-csharp/Gallery.csproj --no-restore --nologo -v:q
```

Each path was built before sampling. Three sequential measurements per case;
every edited sample used a new string value to avoid replaying a previously
compiled source version. Source contents were restored and rebuilt afterwards.
Rust edits changed the `Project workspace` fallback label in
`apps/gallery/src/workspace.rs`; C# edits changed the `Good morning, Alex.` label
in `apps/gallery-csharp/Components.cs`. The framework case changed an error
string in `crates/plico/src/calendar.rs`, without changing public signatures.

| Case | Samples (seconds) | Median |
| --- | --- | --- |
| Rust, unchanged | 0.302 / 0.316 / 0.303 | 0.303 s |
| Rust, application UI label edit | 6.285 / 4.041 / 4.147 | 4.147 s |
| Rust, framework implementation edit | 21.068 / 4.368 / 4.442 | 4.442 s |
| C#, unchanged | 0.983 / 0.924 / 0.857 | 0.924 s |
| C#, application UI label edit | 2.228 / 1.286 / 1.382 | 1.382 s |

The initial Rust warmup took 12.877 s. First edited samples were also slower;
three samples do not characterize tail latency or fully cold caches. Framework
edits triggered compilation of Plico and its downstream workspace crates, even
though repeated edits benefited greatly from incremental reuse. This is a narrow
implementation edit, not a public API change benchmark.

For these two existing galleries, managed UI edit/build latency was approximately
three times lower by median, while Cargo's unchanged-build check was faster.
The applications are not identical, so this is a development-workflow diagnostic,
not a language-level compiler comparison. It does not measure hot reload, first
visible frame, Go, NativeAOT, release builds, or changes to the Rust native library
used by C#. Generated raw results and logs remain under `reports/compile-speed/`.
