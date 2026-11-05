# Lavapipe 与 tiny-skia 软件窗口对照

2026-11-06。本机历史实验，不是对两个库的通用排名。

**当前状态**：tiny-skia 软件后端已撤回，不保留 software feature 或其依赖。
下面保留当时的对照结果；现在的 `wgpu-software-probe` 只用于复测 wgpu 软件 adapter。
历史 tiny-skia 数值不表示当前仓库仍提供该路径。

## 环境与方法

- Linux / X11 Xvfb，800×700，scale 1，Intel Core Ultra 9 285K，24 个可用逻辑 CPU。
- Mesa 25.0.7、LLVM 19.1.7。固定 Vulkan ICD 后，wgpu 日志确认
  `device_type: Cpu`、`backend: Vulkan`、`llvmpipe (LLVM 19.1.7, 256 bits)`。
  Lavapipe 使用 llvmpipe 栅格化基础设施，所以名称显示 llvmpipe 不表示走了 OpenGL。
- 相同 release 二进制，同时编译 software、wgpu、profiling；无其他构建任务并行。
- 当时的 `software-comparison` 示例共用布局/状态代码。通过注入 Clock 驱动动效，
  使用 deadline 将两条路径的目标频率统一限制在 60 Hz，避免 Xvfb 的 FIFO 实际不节流。
- 每次窗口首次提交后预热 1.5 秒，再测 3 秒；每条件两轮，第二轮反转后端顺序。
  系统字体/驱动磁盘缓存没有清空，不是 cold-cache 启动测试。
- CPU 从 `/proc/<pid>/stat` 统计进程所有线程的 user+system 时间，按 24 核容量折算。
  单核跑满显示约 4.17%，而非 100%。不包含 Xvfb/系统合成器进程，也没有测功耗。
- RSS 为采样期平均驻留内存。帧耗时来自 `frame.complete` 的 `present_ms`；它包含
  当前后端的绘制/提交/等待，不是 GPU timestamp，也不是 input-to-photon 延迟。
- 普通文字遵循各后端当前默认实现：wgpu 为 Pango provider，software 为 Parley +
  native positioned glyph。额外无文字 geometry 对照排除了这项差异。

## 结果

两轮平均；p50 列是各轮 p50 的平均。FPS 为窗口实际完成的帧数，不用目标频率代替。

| 场景 | 后端 | 实际 FPS | 应用 CPU / 24 核 | RSS MiB | 绘制/提交 p50 ms |
|---|---|---:|---:|---:|---:|
| 静止 | tiny-skia | 停帧 | 0.00% | 40.6 | — |
| 静止 | Lavapipe | 停帧 | 0.00% | 155.1 | — |
| 无文字：3 个进度条 + 24 个透明度动效圆点 | tiny-skia | 16.0 | 4.10% | 35.4 | 61.48 |
| 同上 | Lavapipe | 51.5 | 41.49% | 185.8 | 16.25 |
| 上述动效 + 静态文字列表 | tiny-skia | 16.0 | 4.13% | 43.8 | 62.08 |
| 同上 | Lavapipe | 50.6 | 41.93% | 212.4 | 17.29 |
| 文字列表滚动，进度条/圆点静止 | tiny-skia | 58.5 | 1.20% | 40.7 | 4.39 |
| 同上 | Lavapipe | 57.1 | 6.37% | 161.6 | 5.04 |
| 透明度动效 + 文字滚动 | tiny-skia | 15.8 | 4.11% | 43.9 | 62.55 |
| 同上 | Lavapipe | 48.6 | 40.09% | 212.6 | 18.29 |

透明度动效场景中，两者帧率不同，不能只根据 CPU 百分比宣称 tiny-skia 更省。
按进程 CPU 时间/完成帧折算，静态文字+动效约为 tiny-skia 62 ms CPU/帧、Lavapipe
199 ms CPU/帧；Lavapipe 用并行执行换取了更低的墙钟时间。单纯滚动两者帧率接近，
CPU 时间约为 4.9 与 26.8 ms/帧。

从 main 的 `probe.start` 到首个 `frame.complete`，滚动场景约为 tiny-skia 82 ms、
Lavapipe 171 ms。它不是显示器已经显示像素的时刻，也不包含进程动态加载之前的时间。

## 线程数补充实验

`LP_NUM_THREADS` 是本次驱动环境下的实验变量，没有改变 Plico 的默认设置。
每项只跑一轮，其他设置相同。

| Lavapipe 线程设置 | 透明度动效 FPS | 应用 CPU / 24 核 | 滚动 FPS | 应用 CPU / 24 核 |
|---|---:|---:|---:|---:|
| 1 | 10.3 | 4.04% | 55.7 | 2.79% |
| 2 | 16.9 | 7.18% | 56.6 | 3.27% |
| 4 | 23.8 | 13.87% | 58.3 | 3.44% |
| 8 | 34.4 | 24.06% | 58.0 | 4.87% |
| 驱动默认（主对照两轮） | 50.6 | 41.93% | 57.1 | 6.37% |

限制线程数可降低 CPU，但重动画的帧率也下降，没有免费获得相同性能。

## 已验证能力与解释边界

- Lavapipe 可以运行现有 wgpu renderer，普通文字、裁剪和透明图层可呈现。
- 单独的 shader workload 成功显示并动画化自定义 WGSL 波纹，没有 shader/设备错误。
  该项只用于能力验证，不把它与 tiny-skia 省略效果后的画面做性能比较。
- 静止时两者均停止重绘；Lavapipe 不意味着必然持续消耗 CPU。
- 无文字对照依然呈现 tiny-skia 约 61 ms 的绘制时间，因此这项瓶颈不能归因于文字。
  资源日志显示 wgpu 动效场景有 24 个离屏图层、25 个 pass，图层工作像素约 51 MiB/帧；
  tiny-skia 初版同样为每个透明图层分配整窗口位图。局部图层尺寸/透明度处理是明确的
  后续优化候选，但本实验没有用 CPU 栈采样证明具体函数占比。
- 这不是两个后端已经充分优化后的比较，尤其不能把 tiny-skia 初版的图层开销当作
  tiny-skia 库本身的性能结论。也不从这几秒的 CPU 时间推断实际电池续航。

## 重现

```sh
cargo build --release -p plico-desktop --no-default-features --features wgpu,profiling --example wgpu-software-probe
```

Linux 固定 Lavapipe（ICD 路径依发行版而异）：

```sh
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json WGPU_BACKEND=vulkan \
RUST_LOG='info,plico::perf=trace' \
cargo run --release -p plico-desktop --no-default-features --features wgpu,profiling \
  --example wgpu-software-probe -- --workload animation --seconds 10
```

当前不能复测已撤回的 tiny-skia 实验路径。`--workload` 支持 `idle`、`geometry`、
`animation`、`scroll`、`both`、`shader`。Idle 不自动关闭，请手动关闭窗口。
有窗口服务器时直接运行；无显示环境时外层加 `xvfb-run -a`。
检查 adapter 日志，不能把硬件 GPU 结果误记为 Lavapipe。

测进程 CPU 时在首帧后预热，再按 `getconf CLK_TCK` 将 `/proc/<pid>/stat` 的
utime+stime 差值转为 CPU 秒，除以采样墙钟时间和可用 CPU 数量。不要用 shell/
cargo 的累计启动 CPU 代替常驻应用采样。截图、逐帧日志与原始采样保留在本地 reports，
不参与提交。
