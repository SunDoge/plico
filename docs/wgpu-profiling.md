# wgpu 窗口 CPU 阶段采样

真实设置页可同时输出共享运行时计数与 wgpu 阶段耗时：

```sh
mkdir -p reports/wgpu-scene
PLICO_WGPU_DAMAGE=1 PLICO_PROFILE=counters \
PLICO_TELEMETRY_FILE=reports/wgpu-scene/settings-telemetry.json \
RUST_LOG=plico::perf=debug \
cargo run --locked -p backend-settings --features wgpu,profiling
```

切换侧栏、导航或编辑后正常关闭窗口，退出 guard 写出 JSON。单窗口和多窗口
入口都安装该 guard；强制终止进程不会运行正常退出导出。截图、JSON 和日志
属于本地产物，不提交。生产性能判断应再使用 release 构建和真实桌面复测。

调试级别的 tracing span 分开记录：

| Span | 范围 |
| --- | --- |
| `wgpu.prepare_scene` | 共享命令转换、原生几何与准备缓存；包含缓存命中路径 |
| `wgpu.damage` | 原生 ink 边界下的差异与脏区计算 |
| `wgpu.encode` | 资源准备与绘制命令编码，包含内部上传与特效准备 |
| `wgpu.submit` | 最终 blit 编码、command buffer 完成和 queue submit |
| `wgpu.readback_including_gpu_wait` | 离屏像素回读，明确包含 GPU 等待与映射 |

窗口路径不应出现最后一项。`time.busy` 是 CPU 侧作用域的墙钟时间，可能包含
驱动等待，不是 CPU cycles 或 GPU timestamp。父子 span 可能嵌套，不能把全部
span 相加。改用 `RUST_LOG=plico::perf=trace` 可查看资源与 damage 明细；未启用
trace 时，不提前遍历缓存计算这些统计。Windows 资源记录还包含字体原生复制
字节数、覆盖率栅格化与 CPU 着色次数。

共享 JSON 的 `summaries` 给出 `frame/retained.cpu`、`present.cpu` 和 `redraw.cpu`。
前者覆盖共享构建、布局和 Scene 生成，不能解释成仅执行 `show`。`present.cpu`
还包括窗口获取/呈现与可能的驱动等待；它不是纯编码时间。百分位使用最近
128 个样本，`count`、`totalUs` 则累计整个录制过程。

## 本机采样

Linux/X11、Xvfb、NVIDIA RTX 5080/Vulkan，1100×820 设置页，启用 damage、计数器
和 span 日志；dev profile 的 `opt-level=1`。侧栏切换 8 次，每次间隔约 400 ms，
之后正常关闭。最近 128 次调用的结果如下：

| 阶段 | p50 | p95 |
| --- | ---: | ---: |
| 共享 `retained.cpu` | 2208 µs | 2856 µs |
| wgpu 场景准备 | 64.3 µs | 145 µs |
| wgpu damage | 19.2 µs | 33.1 µs |
| wgpu 编码 | 489.5 µs | 964 µs |
| wgpu 提交 | 64.3 µs | 75.6 µs |

共享窗口录制 466 次 redraw/retained/present 调用；场景准备/damage 各 466 次，
编码/提交各 465 次。仅凭计数不判定少一次调用的具体原因。没有出现回读 span。
记录器报告累计自身记录开销
3187 µs；这不包含全部 tracing 格式化或采样干扰。

该样本支持下一步优先细分共享构建和编码开销，不能证明某个具体函数已经是
热点，也不能据此推导正常桌面 CPU 使用率、GPU 利用率、电池消耗或跨后端排名。
Xvfb 没有真实显示器的 VSync/合成器条件。不同阶段调用数及样本窗口可能不同，
中位数也不能相加当作整帧中位数。完整 `present.cpu` 的 p50 为 2162 µs，明显
包含上表编码/提交之外的窗口工作，需要单独继续定位。

## 已移除的重复资源创建

Retained render target 现在持有自己的 texture view，稳定尺寸的绘制不会每帧
重新创建它。最终 blit 只缓存一个源 view/bind group；源身份变化时替换，根目标
resize 时释放旧绑定，输出格式变化时重建 blitter。缓存不持有窗口 swapchain
的 destination view，因此不会跨帧复用已取得的窗口图像。

GPU 像素测试覆盖稳定目标的内容更新、目标尺寸变化，以及 RGBA/BGRA 和 sRGB
切换；view 身份断言还检查了同尺寸复用、resize 后替换。这消除了明确的逐帧
资源创建，但尚未通过稳定的 release 前后采样量化 CPU 收益，不能套用上表
旧样本宣称加速比例。

窗口 `trim_gpu_cache()` 现在也清理 immutable-image GPU 缓存及最后一帧的绑定引用；
之前仅清理 layer/effect/text，图片缓存仍可保留至 128 MiB。独立
`Renderer::trim_image_cache()` 保留累计上传／命中计数，resident bytes 与 binding
数量归零。它不清理应用／scene 持有的 CPU 原图，也不破坏最终 retained pixels。
GPU 测试验证 trim 后静态画面无需重传，随后图片移动会重新上传并与全量重绘一致。
统计归零表示 renderer 已释放引用，驱动仍可能为在途提交暂留资源，不能等同于 RSS
立即下降。

`Renderer::raster_cache_stats()` 另提供 path/SVG/clip-mask 三组 CPU 缓存的
`(entries, payload_bytes)`。`trim_raster_cache()` 清理这些缓存并丢弃 renderer
本地 prepared snapshot，避免后者继续持有相同光栅数据；窗口 trim 已接入。
外部调用方仍持有的 prepared snapshot 和应用源资源不在此释放范围。
嵌套圆形裁剪＋路径＋SVG 的 GPU 测试验证三类缓存归零、后续重新准备，以及
强制完整绘制后的像素一致性。trim 可能将光栅工作推迟到下一次 scene preparation，
因此它是内存回收入口，不应每帧调用。

根帧编码结束（成功或失败）也会立即清空 pass-local 图片 binding intern 表。
命令编码器持有提交所需 GPU 引用，不需要 renderer 再保留到下一次编码；这可避免
LRU 已淘汰的纹理被上一帧临时表继续持有。缓存自身仍按正常策略复用资源。
GPU 测试覆盖成功提交、第二张图片上传失败及随后恢复，并验证帧表为空；嵌套
layer/effect/batching 的 17 项 GPU 回归通过。未据此宣称 RSS 降幅。

### 资源回收改动后的窗口回归

Linux/X11、Xvfb、NVIDIA Vulkan，`wgpu-window`、1.25× scale、damage 开启：
实际点击计数、输入 `Cache regression`、启停 WGSL 动画，并将窗口从 775×600
调至 1000×760 再恢复。检查截图时状态保持，未观察到拉伸或旧像素残留。
正常 `WM_DELETE_WINDOW` 关闭另行确认 exit code 0。此场景未主动触发 trim，
trim 的正确性由前述 GPU 测试覆盖；不将窗口截图当作内存回收的证据。
Xvfb 报告缺少 DRI3，不能据此评价真实合成器下的延迟、帧率或功耗。
生成截图与日志留在本地 reports/ 或临时目录，不进入仓库。

截图 readback buffer 也纳入窗口 trim。`target_storage_bytes()` 分别返回根颜色目标
与可选 readback buffer 的实际载荷字节（后者包含 256-byte 行对齐）。
`trim_readback_buffer()` 保留颜色目标，随后正常窗口编码不会重建 readback；下一次
捕获才按需分配。GPU 测试覆盖这条生命周期及重捕获像素一致性。
TRACE 资源事件新增 `target_color_bytes`、`readback_buffer_bytes`，以及
`path_raster_bytes`、`svg_raster_bytes`、`clip_raster_bytes`；所有统计仍在 TRACE
启用检查内，默认不计算或输出。这些值不包含 swapchain、驱动开销或在途资源。

## CPU mip generation

A specialized even 2×2 reduction now replaces general area-weight arithmetic for
that case. Both use the same rounding; complete-chain byte comparisons cover
square, rectangular, odd-start and one-axis-terminal dimensions. Odd reductions
retain exact area weights.

```sh
cargo test --locked -p plico-wgpu --lib profile_mip_area_cost -- --ignored --nocapture
```

Local Intel Core Ultra 9 285K, dev opt-level=1, 4096×3072 RGBA, seven samples:
general weighted complete-chain median 90.309 ms (89.874–97.782), specialized
median 7.535 ms (7.132–8.140). This isolated test includes output allocations but
excludes source creation, decode, conversion, GPU upload and waits. It is not a
renderer frame-time result or release benchmark. First generation remains
synchronous; especially large odd-sized input may still cause upload-time jank.
The profile is ignored by default and has no wall-time pass threshold.
Fifteen image/affine GPU regressions and all-target Clippy passed after the change.

Odd mip reductions now precompute each destination axis's source offsets and
integer overlap weights. A mip step touches at most three pixels per axis;
plans occupy O(width + height) temporary memory and retain the same single final
rounding. A separate test-only per-pixel reference remains, checked against 561
small dimension pairs and multiple complete chains, including one-pixel axes.

The same manual profile now measures both 4096×3072 and 4095×3071. In a follow-up
seven-sample dev run on the same CPU, the odd complete-chain reference median was
143.812 ms (143.521–144.625), optimized 78.402 ms (78.022–78.648). Even dimensions
remained 7.035 ms. This confirms a useful reduction, but 78 ms synchronous work
is still a visible first-upload risk. GPU generation or background preparation
requires further work; this optimization is not the final upload pipeline.
Fifteen image/affine GPU regressions, Clippy and the WASM check passed.

## Upload CPU work versus subsequent GPU wait

```sh
cargo test --locked -p plico-wgpu --lib profile_image_upload_cpu_and_gpu_wait -- --ignored --nocapture
```

This manual test uploads distinct large premultiplied RGBA sources, comparing a
test-only forced CPU mip path with the production GPU path. Source construction
and adapter creation are outside timing. The CPU interval includes conversion,
texture creation, staging writes, mip work/submission and an empty staging flush;
the separately timed device-idle wait follows it. The first upload per renderer
is reported separately; six later distinct uploads reuse the pipeline, not image
cache hits. Reported statistics are upper medians for six samples, not GPU
timestamps. Driver shader caches may already be warm.

Local Intel ARL/Mesa Vulkan, dev opt-level=1: before borrowing premultiplied input,
GPU-path CPU upper medians were 48.733 ms (4096×3072) and 51.139 ms (4095×3071).
Avoiding Plico's redundant full-image copy/conversion walk reduced them to 32.237
and 30.664 ms. CPU-mip fallback after the same change was 41.534 / 111.135 ms.
Subsequent GPU waits varied (6.237 / 10.172 ms for those GPU-path runs); these are
not isolated mip GPU execution times, nor may separate medians be added to infer
a frame median. The result is not a release/power benchmark.

The borrow only applies to already premultiplied RGBA. Straight-alpha or BGRA
inputs retain conversion, and wgpu still stages/copies upload bytes. CPU fallback
versus GPU pixel comparisons cover both formats and alpha representations. Large
first uploads remain expensive; this is not a claim of zero-copy or jank-free
image loading. The test override is absent from production builds.

## Upload stage spans

Enable `RUST_LOG=plico::perf=debug` on the manual upload profile above to emit
close-time spans (a test-only tracing subscriber reads that filter). Production
apps use their own subscriber. The subscriber is a dev dependency only; the lock
file records it. Cache hits return before creating upload-stage spans.

Stages are `wgpu.upload_image`, `wgpu.prepare_image_bytes`,
`wgpu.allocate_image`, `wgpu.stage_image_level`, `wgpu.cpu_mip`,
`wgpu.compile_mip_pipeline`, and `wgpu.generate_mips`; the latter additionally
contains `wgpu.finish_mips` and `wgpu.submit_mips`. All are CPU wall intervals.
Fields identify dimensions, source/payload bytes, format/alpha, mip level and
GPU/CPU selection. Existing INFO behavior does not enable these DEBUG stages.

A follow-up Intel/Mesa dev sample (seven distinct uploads including first use)
located the long interval at queue submission: mip-submit medians 22.4 / 21.7 ms
for even/odd 4K inputs, versus encoder-finish 0.141 / 0.125 ms. First pipeline
creation was 0.481 / 0.474 ms. An earlier staged sample showed occasional stalls
moving between staging write and submission. These observations identify a CPU
API boundary, not GPU shader execution duration or proof of a driver defect.
Staging, driver work and synchronization inside that boundary remain unresolved.
Do not sum nested span times or separate medians. No upload-speedup claim follows
from adding instrumentation.
