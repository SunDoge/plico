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
