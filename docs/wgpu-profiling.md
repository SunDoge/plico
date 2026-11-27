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

当前 `summaries` 给出 `frame/build.cpu`、`present.cpu` 和 `redraw.cpu`。下表沿用当时采样的 `retained.cpu` 名称。
前者覆盖共享构建、布局和 Scene 生成，不能解释成仅执行 `show`。`present.cpu`
还包括窗口获取/呈现与可能的驱动等待；它不是纯编码时间。百分位使用最近
128 个样本，`count`、`totalUs` 则累计整个录制过程。

## 本机采样

### Liquid Glass Showcase

使用 release 版 Gallery，进入 Liquid Glass Showcase 并开启自动播放。关闭 HUD，
用 `RUST_LOG=info,plico::perf::frame=trace,plico::perf::surface=trace`
记录每次成功提交和取图、呈现阶段的耗时。默认关闭 `PLICO_GPU_TIMER`，
测设备执行时间时单独开启，以免把查询开销混入帧率对照。

在 59.98 Hz 的 KDE Wayland 桌面，Intel ARL / Mesa 25.0.7，
逻辑 1920×1080、缩放 1.75 的窗口中，同一构建各测 12 秒。FIFO 呈现
保留窗口的帧回调时提交 30.3 FPS。仅在实验中取消该回调时为 55.0 FPS。

| 阶段 | 默认 P95 | 取消帧回调 P95 |
| --- | ---: | ---: |
| 成功提交之间的间隔 | 53.0 ms | 26.7 ms |
| UI 构建 | 3.75 ms | 3.31 ms |
| Scene 生成 | 0.69 ms | 0.71 ms |
| 交换链图像获取 | 0.08 ms | 0.12 ms |
| surface present 调用 | 0.26 ms | 13.41 ms |

逐帧扣除 UI 构建、Scene 生成和呈现阶段后，默认路径的间隔中位数还余
20.2 ms。实验路径的余量中位数为 0.06 ms，部分等待转移到了
FIFO 的 present 调用。这里统计成功提交的节奏，显示器扫描时刻需要
合成器反馈另行测量。这个对照中的默认路径是修改前的 FIFO 调度。

另一次实验保留显示器节奏，用刷新周期定时并采用 Mailbox，得到 58.1 FPS、
23.9 ms 的 P95 帧间隔。MyGO 的 Linux 路径按当前显示器刷新周期推进 GTK frame
clock，并由 GTK 在窗口可见时触发绘制（`internal/linux/surface.go`，
`74c3f26d8725e839a611e7ce0ef38388a906db20`）。

随后使用 Mailbox 的原生 Wayland 窗口同时使用显示器刷新周期和最近的
合成器重绘事件。后者停止时，定时绘制也会停止。在同一台机器上，
24 秒自动播放的 1120×820 默认窗口为 59.8 FPS、P95 23.0 ms，
1920×1080 窗口为 57.1 FPS、P95 26.3 ms。两个窗口均为 175% 缩放。
默认窗口最小化后，连续 3 秒没有新提交，恢复后重新开始绘制。

大窗口另一次 1,379 帧样本的 `plico::perf::damage=trace` 计时为
P95 0.147 ms、最大 0.230 ms。UI 同步 P95 0.155 ms、最大 0.519 ms。
其中 774 帧仍走全量绘制，剩余长帧优先分析 wgpu 场景准备及提交。

虚拟时钟可以按 120 或 240 Hz 采样同一个页面，无需录视频或读回像素：

```sh
PLICO_GPU_TIMER=1 cargo run --locked --release -p gallery --features profiling \
  --example showcase_profile -- --fps 120 --width 1920 --height 1080 --scale 1
```

将 `--fps` 改为 `240` 即使用 4.17 ms 预算。离屏工具逐帧等待 GPU 完成，
报告 CPU 提交、GPU 时间戳和完成耗时。真实窗口可以让 CPU、GPU 和合成器并行，
因此离屏的完成耗时需要与实屏呈现节奏分别解读。

1920×1080、100% 缩放、开启 GPU 时间戳的首轮数据如下。每档预热 1 秒，
采样 5 秒自动播放。表中的 CPU 时间包括应用构建、渲染命令准备和提交。

| GPU | 虚拟刷新率 | CPU 到提交 P95 | GPU P95 | 串行完成超预算 |
| --- | ---: | ---: | ---: | ---: |
| Intel ARL | 120 Hz | 18.0 ms | 14.8 ms | 529 / 600 |
| Intel ARL | 240 Hz | 18.5 ms | 12.8 ms | 1198 / 1200 |
| RTX 5080 | 120 Hz | 12.1 ms | 1.26 ms | 183 / 600 |
| RTX 5080 | 240 Hz | 23.1 ms | 1.59 ms | 919 / 1200 |

RTX 5080 的 GPU 执行有余量，CPU 长帧仍超出两个预算。Intel ARL 的 GPU
执行也需要进一步优化。高刷新率的实屏结果暂无。

开启 `PLICO_TRACE_FILE` 和 `PLICO_TRACE_FILTER=plico::perf=debug` 可以把
离屏流程的 CPU 调用写入 Chrome trace。RTX 5080 上的 600 帧追踪中，
wgpu 场景准备 P95 为 8.28 ms，同一帧内累计的原生段落准备 P95 为
7.55 ms。路径栅格、SVG 和遮罩准备的累计 P95 分别为 0.26、0.12 和
0.02 ms。窗口调度、场景准备和 GPU 执行分开测量。

将场景的实例和帧 uniform 上传打包成单次写入并额外编码缓冲区拷贝的方案
已撤回。同一构建打开开关后，RTX 5080 的 CPU 提交 P95 从 11.9 升到
12.9 ms，Intel ARL 从 15.2 升到 16.0 ms。

文本栅格密度跟踪参考 MyGO 的 `ui/rasterscale.go`
（`74c3f26d8725e839a611e7ce0ef38388a906db20`，MIT）。
变换连续变化时，相邻尺度共用每倍频程八档中的同一档。最终绘制变换保持
实际尺度，停止变化后补绘一帧精确栅格。测试覆盖非等比缩放的档位复用和
保留 Scene 路径的最后一帧。

更新后，同一台 Wayland 机器的大窗口（1920×1080、175% 缩放）
在 25 秒自动播放中提交 59.9 FPS，帧间隔 P95 从上一轮的 26.3 ms 降至
21.8 ms，整个呈现阶段 P95 从 21.9 降至 12.1 ms。两个样本都使用按刷新周期的
调度。1080p 100% 缩放的 RTX 5080 离屏测试中，7 秒 A/B 的 CPU 提交 P95 从
14.4 降至 12.4 ms，同样的分阶段追踪短跑则仍约为 8.3 ms 场景准备、
7.6 ms 段落准备。优化收益随窗口、负载阶段和采样方式变化。

这次追踪发现局部 Scene 更新没有复用文字的缩放状态。把同一份状态接入该路径后，
静止的文字可以沿用已有片段，使用了近似栅格的片段会在最后一帧重新以精确密度
绘制。Linux 的纯绘制请求也改为只准备字形，测量实际需要宽度和行高时才计算
Pango 的固有尺寸。

| GPU 和物理尺寸 | 虚拟刷新率 | CPU 到提交 P95 | 串行完成 P95 | 超预算 |
| --- | ---: | ---: | ---: | ---: |
| RTX 5080，1920×1080 | 120 Hz | 3.55 ms | 4.82 ms | 15 / 720 |
| RTX 5080，3360×1890 | 120 Hz | 4.47 ms | 6.41 ms | 16 / 960 |
| RTX 5080，1920×1080 | 240 Hz | 3.86 ms | 5.41 ms | 407 / 1920 |
| Intel ARL，1920×1080 | 120 Hz | 7.75 ms | 24.71 ms | 735 / 960 |

这些样本关闭 GPU 时间戳。1080p RTX 5080 上，同轮修改前后的 120 Hz 样本
均为 720 帧，CPU 到提交 P95 从 9.59 降至 3.55 ms，串行完成超预算从
124 降至 15 帧。高刷新率实际呈现仍待实屏验证。

当前构建也在 Xvfb 的 1920×1080 窗口跑完整自动播放，RTX 5080 的 12 秒样本
平均提交 131.5 FPS，帧间隔 P95 为 13.6 ms。这一路径可以检查窗口和交互，
没有物理显示器的同步条件。此次物理机处于锁屏，实屏一栏沿用前一次解锁时的
测量。

### 玻璃背板和局部重绘

Intel ARL 在 3360×1890 的诊断样本中，保留全部效果时 GPU P95 为
15.6 ms。临时跳过前景高斯滤镜时为 13.6 ms，临时跳过读背板的玻璃材质时
为 10.4 ms。各组独立运行，能反映主要开销的位置，无法用差值直接分摊
每个 GPU pass。该诊断样本还包括旧版帧计时器对每个高斯工作区的额外查询。
应用帧计时现只解析整帧查询，滤镜的逐 pass 查询由单元基准使用。

带背板的透明度分组现在可以局部重绘。GPU 测试覆盖更新分组背后的内容和
只改远处的情况，前者与全量重绘逐像素相同，后者跳过背板拷贝。
另以同一个 Showcase 的 150 帧做逐像素对照，差异为零。

3360×1890 的 360 帧、关闭计时查询的 A/B 中，集显串行完成 P95 从
24.9 降到 23.6 ms，超过 16.67 ms 的帧数从 133 降到 111。1920×1080
的重复测量中，前后全量重绘均为 202/360，整体帧时没有稳定提升。
另做了预计算玻璃卡片采样位置和权重的实验，同轮 P95 由 24.7 升到
26.2 ms，已恢复原采样代码。

最终构建开启整帧时间戳后，Intel ARL 的 GPU P95 在 1920×1080 为
13.6 ms，在 3360×1890 为 14.9 ms。高 DPI 样本的串行完成 P95 为
23.5 ms，实屏因系统仍在锁屏，本轮无有效数据。

固定高斯背板现在可以复用上一帧已经提交的结果。wgpu 用实际捕获范围
检查前序绘制的局部变化，同时核对命令位置、捕获坐标和滤镜参数。
在 3360×1890、60 Hz 的两轮同一构建对照中，360 帧的背板拷贝从
4,550 降到 3,672，预滤波 pass 从 6,161 降到 5,723。集显 GPU P95
从 15.3–15.5 ms 降到 14.7–14.8 ms，串行完成 P95 为 24.1–24.3 ms，
仍有超过 16.67 ms 的帧。设置 `PLICO_FORCE_BACKDROP_REFRESH=1` 可关掉
跨帧复用，方便复现对照。

100% 和 175% 缩放各取 150 帧，局部重绘加跨帧缓存的输出与强制全量
重绘逐像素相同。GPU 回归测试还分别覆盖输入区内的变化、区外的变化、
材质自身随时间变化和捕获位置移动。

高斯采样的边界映射现在只计算当前模糊轴。Intel ARL 在 3360×1890
物理像素下的两轮 300 帧离屏 A/B 中，GPU P95 从 12.21、14.35 ms
降到 11.77、12.06 ms，背板拷贝与预滤波 pass 数量相同。无法使用整块
预滤波的边缘尺寸约占背板源像素的 1%，集中在少数帧。

同一优化在 59.98 Hz Wayland 实屏上自动播放 24 秒。175% 缩放下，
1120×820 逻辑像素窗口平均提交 59.99 FPS，提交间隔 P95 为 20.07 ms。
1920×1080 逻辑像素窗口平均提交 59.77 FPS，提交间隔 P95 为 21.00 ms。
这组数据统计应用提交。下一节用合成器反馈统计实际显示。

模糊工作区复用未变的轴参数，固定背板在创建工作区时初始化降采样因子。
这会减少重复的 GPU 缓冲区上传，拷贝背板和运行滤镜的次数不变。3360×1890、
虚拟 120 Hz 的 600 帧离屏测量使用 wgpu 30.0.1。RTX 5080 的 CPU 提交 P95 从 4.97 降至
4.89 ms，GPU P95 为 1.15–1.16 ms。Intel ARL 两轮 GPU P95 从
16.65、14.98 ms 变为 19.22、13.65 ms。这组数据未测到稳定的帧时间收益。

MyGO `74c3f26d` 的 Linux OpenGL 路径同样执行背景区域拷贝、方块降采样和
两遍高斯模糊（`internal/gpu/gl/gl.go::readBackdrop`）。窗口时序由
GTK 驱动（`internal/linux/surface.go`）。两边的示例有不同的页面结构和动画，
目前没有同硬件、同负载的实测可用于比较掉帧。

### 独显高刷新率采样

固定高斯背板按 2、4、8 倍降采样因子专用化计算管线。整块的固定路径
省去循环中的尺寸截断和像素计数，保留原来的像素累加顺序和舍入。
RTX 5080 上，100% 和 175% 缩放各 150 帧的完整 Showcase 输出，
与专用化前逐帧像素哈希相同。

RTX 5080 Vulkan、3360×1890 物理像素，wgpu 30.0.2 release 构建。
每轮取 5 秒自动播放，CPU 和 GPU 串行完成的时间包含等待，不等同于
真实显示器的帧间隔。

| 虚拟刷新率 | 方案 | CPU 提交 P95 | GPU P95 | 串行完成 P95 | 串行超预算 |
| --- | --- | ---: | ---: | ---: | ---: |
| 120 Hz | 改动前，两轮 | 4.37–5.05 ms | 1.16–1.17 ms | 6.21–6.64 ms | 19–20 / 600 |
| 120 Hz | 专用化，两轮 | 3.53–4.58 ms | 1.15–1.16 ms | 5.33–5.80 ms | 15–16 / 600 |
| 240 Hz | 改动前 | 5.53 ms | 1.19 ms | 7.43 ms | 381 / 1,200 |
| 240 Hz | 专用化 | 4.25 ms | 1.17 ms | 6.08 ms | 251 / 1,200 |

额外的 240 Hz CPU 追踪用独立 span 区分命令缓冲区完成和队列提交。
`encoder.finish()` P95 为 1.65 ms，`queue.submit()` 为 0.17 ms，
前面的渲染命令编码为 1.35 ms，场景准备为 0.94 ms，damage 为 0.10 ms。
这些是带追踪开销的分段百分位，不能相加估算整帧 P95。

本机 60 Hz Wayland 输出无法用 NVIDIA ICD 单独创建 Vulkan 窗口。
开启高性能偏好后窗口仍选用 Intel ARL。因此这里的 RTX 结果仅来自离屏。

固定背板的降采样和两轴模糊现在在同一个计算 pass 中按原顺序派发。
各派发仍写入原有的中间纹理，采样和舍入规则相同。RTX 5080、3360×1890
物理像素下，关闭 GPU 计时的 240 Hz 两轮对照中，串行完成 P95 从
6.40–6.68 ms 变为 6.14–6.45 ms，超预算帧由 253–263 / 960 变为
234–260 / 960。120 Hz 的单轮对照中，P95 从 7.93 降到 5.82 ms，
超预算帧从 20 / 480 降到 15 / 480。100% 和 175% 两种缩放各取 150 帧，
合并计算 pass 前后的输出哈希逐帧一致。

另一次实验为每个玻璃材质保留上次的纹理绑定。240 Hz 对照中超预算帧
增加，已撤回。当前的优化局限于 wgpu 后端的命令组织。

### 独显 X11 窗口采样

合并计算 pass 后重新构建 release Gallery，使用独立的 3840×2160 Xvfb
桌面、NVIDIA Vulkan ICD 和 FIFO。窗口自动播放，关闭 GPU 计时和 HUD，
预热 3 秒后取 15 秒。表中统计窗口成功提交，Xvfb 没有物理屏幕。

| 窗口物理像素 | 提交 FPS | 提交间隔 P95 | 间隔超过 16.67 ms |
| --- | ---: | ---: | ---: |
| 1920×1080 | 106.99 | 15.62 ms | 67 / 1,605 |
| 3360×1890 | 56.12 | 26.10 ms | 497 / 842 |

追加窗口阶段追踪后，3360×1890 的 12 秒样本提交 58.70 FPS。交换链
图像获取 P95 为 0.011 ms，`surface present` 调用 P95 为 18.48 ms，
命令编码为 1.52 ms，提交阶段为 1.68 ms。高 DPI 样本的大头在当前
Xvfb 呈现调用。一次同构建的反序对照每轮采样 8 秒，FIFO 的两轮
提交 FPS 为 70.89、55.95，Immediate 为 73.35、71.01。
FIFO 的提交间隔 P95 为 23.49、27.53 ms，Immediate 为 21.78、23.61 ms。
该 Vulkan surface 只提供 FIFO 和 Immediate，正式窗口继续使用 FIFO。

系统调用追踪发现，Xvfb 没有 DRI3，窗口的 X11 socket 每帧发送七条
`PutImage` 请求。256 帧共发送 6,564,741,120 字节的像素 payload，
平均每帧 25,643,520 字节。这条虚拟窗口路径的成本包含整张图像传输。
独显实际显示节奏仍需连接兼容显示输出验证。

### 独显 60 Hz 渲染预算长测

RTX 5080 Vulkan、release、关闭 GPU 时间戳。以 60 Hz 虚拟时钟驱动完整
Showcase 自动播放，预热 1 秒后采样 20 秒。逐帧等待 GPU 完成。

| 物理像素 | CPU 提交 P95 | 串行完成 P95 | 串行完成 P99 | 超过 16.67 ms |
| --- | ---: | ---: | ---: | ---: |
| 1920×1080 | 3.61 ms | 4.94 ms | 8.88 ms | 1 / 1,200 |
| 3360×1890 | 4.03 ms | 5.78 ms | 10.33 ms | 1 / 1,200 |

现有 Linux 会话的唯一已连接输出为 Intel DRM 设备的 DP-1，刷新率
59.98 Hz。RTX 5080 报告显示输出未启用，本机的这组独显数据来自离屏。

### Wayland 实际显示节奏

在同一台 59.98 Hz KDE Wayland 桌面上，给每次窗口 present 请求
`wp_presentation` 反馈，使用合成器返回的时间戳和丢弃事件。GPU 查询关闭，
每个尺寸的自动播放取 24 秒。表中尺寸为逻辑像素，缩放为 175%。

| 窗口 | 模式 | 提交 FPS | 显示 FPS | 丢弃反馈 | 显示间隔 P95 | 提交到显示 P50 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1120×820 | Mailbox + 刷新时钟 | 59.81 | 56.70 | 75 / 1,437 | 33.33 ms | 27.08 ms |
| 1120×820 | FIFO + 刷新时钟 | 59.64 | 59.66 | 0 / 1,432 | 16.68 ms | 48.12 ms |
| 1920×1080 | Mailbox + 刷新时钟 | 59.82 | 56.54 | 79 / 1,436 | 33.33 ms | 27.69 ms |
| 1920×1080 | FIFO + 刷新时钟 | 58.36 | 58.33 | 1 / 1,401 | 16.68 ms | 43.11 ms |

原生 Wayland 宿主现在使用 FIFO，保留按刷新周期调度和合成器重绘活动门控。
FIFO 保留更多已提交的帧，使显示步进更均匀，也会增加排队延迟。单靠帧回调
调度的 FIFO 在另一次 12 秒大窗口对照中只显示 46.81 FPS，同轮 FIFO 加
刷新时钟是 58.66 FPS。默认窗口最小化后稳定的 3 秒内没有新帧，恢复后继续
绘制。大窗口的 24 秒样本仍有 38 个显示间隔落在两个或三个刷新周期，
因此还需要缩短慢帧和排队时间。尚未验证 120 Hz 或 240 Hz 实屏。

### 设置页

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
`wgpu.compile_mip_pipeline`, and `wgpu.generate_mips`; deferred GPU mip work is
finished under `wgpu.finish_mips`. All are CPU wall intervals.
Fields identify dimensions, source/payload bytes, format/alpha, mip level and
GPU/CPU selection. Existing INFO behavior does not enable these DEBUG stages.

Before mip work was merged into the frame submission, a follow-up Intel/Mesa dev
sample (seven distinct uploads including first use) located the long interval at
the then-separate queue submission: mip-submit medians 22.4 / 21.7 ms
for even/odd 4K inputs, versus encoder-finish 0.141 / 0.125 ms. First pipeline
creation was 0.481 / 0.474 ms. An earlier staged sample showed occasional stalls
moving between staging write and submission. These observations identify a CPU
API boundary, not GPU shader execution duration or proof of a driver defect.
Staging, driver work and synchronization inside that boundary remain unresolved.
Do not sum nested span times or separate medians. No upload-speedup claim follows
from adding instrumentation.

The production path now defers newly uploaded textures' mip encoders and prepends
their command buffer to the next successful frame. Child layers and effects are
recorded directly in the root frame encoder; therefore a frame uses one command
buffer normally and two only when it has a mip prefix, all in one `queue.submit`.
Child layers and the final frame sample initialized levels without a hidden
upload-time submit. Pending textures are only
committed after that submit; abandoning a failed surface frame leaves them
retryable. The manual upload profile explicitly submits deferred mip work because
it intentionally bypasses frame encoding. This removes a submission boundary,
but no before/after end-to-end speedup is claimed without a fresh benchmark.

### Base-level upload control

The manual upload profile now includes `base-only`, `cpu-mips`, and `gpu-mips`
under the same source/queue timing protocol. A subsequent dev run on Intel/Mesa
reported CPU upper medians (six post-first samples):

| Source | Base only | CPU mips | GPU mips |
|---|---:|---:|---:|
| 4096×3072 | 33.100 ms | 42.366 ms | 33.483 ms |
| 4095×3071 | 31.474 ms | 112.442 ms | 33.885 ms |

Base-only excludes both mip compute and its pipeline, so the long CPU upload
interval does not require mip generation. GPU mips eliminate most odd-size CPU
prefilter cost in this experiment, while the remaining large-image staging/submit
cost needs separate investigation. Base-only and mip modes have different resident
texture payloads and LRU behavior; these small sample differences are not an
isolated marginal GPU-mip cost. Subsequent waits also differ and are reported by
the harness. An explicit staging-buffer comparison is a reasonable next control;
these results alone do not justify changing render scheduling or blaming the GPU
shader. No production behavior changed in this diagnostic extension.

### Explicit staging buffer control

```sh
cargo test --locked -p plico-wgpu --lib profile_explicit_staging -- --ignored --nocapture
```

This separate manual experiment compares `queue.write_texture` with a newly
allocated mapped-at-creation COPY_SRC buffer plus `copy_buffer_to_texture`.
Both time texture allocation, source-data copying and submission; odd row sizes
are padded to 256 bytes on the explicit path. First/middle/last texels are copied
back and checked exactly. Source creation and the tiny oracle buffer allocation
are outside timing. The post-submit interval includes GPU completion/map callback,
not just transfer GPU time. No production upload policy is changed.

On a follow-up Intel/Mesa dev run, upper medians of each trial's paired CPU+wait
total were 38.974 / 38.450 ms for write_texture (even/odd dimensions), versus
46.727 / 46.324 ms for explicit staging. These totals are computed per sample
before sorting, not by adding independently computed medians. Another run showed
no consistent CPU-only benefit either. The simple explicit-buffer replacement
therefore has no demonstrated benefit here and is not adopted. Reused upload
pools, other drivers, release profiles and real app concurrency are not evaluated
by this experiment. The diagnostic remains ignored by default, with no timing
threshold; Clippy passed.

## Class shader specialization

Plico carries MyGO's ordinary `ClassPlain`, `ClassFill`, `ClassShadow`,
`ClassGlyph`, and `ClassImage` split. Plico's atlas distinguishes monochrome and
color glyph instances, so they have separate small pipelines rather than MyGO's
subpixel class.
Gradients and bordered fills use a static `ClassFill` fragment pipeline containing
only clip/shape coverage, gradient interpolation and border composition. Runtime
materials and backdrop markers remain on the general pipeline; outer and inset
box shadows use an analytical shadow-only vertex/fragment pair, glyph pipelines
retain only atlas sampling/tint/clip inputs, and images retain only sampling,
rounded coverage and clip inputs.
Small adjacent runs of different classes still use the existing
`joinLast` threshold and merge into the general pipeline, avoiding extra draw calls.
`Renderer::force_general_pipelines(true)` provides the pixel oracle, and
Per-class `FrameBatchStats` counters prove which specialized paths were selected.
The class pipelines also use dedicated vertex entry points and fragment input
structures: `ClassPlain` does not export the second color, border, configuration,
UV or gradient varyings, `ClassFill` does not export image UVs, and glyphs do not
export shape fields. Coverage masks remain clip state and can use `ClassPlain`;
they do not force a solid into the larger fill class. This follows MyGO's paired
class vertex/pixel shaders instead of relying on a backend compiler to discover
that the general vertex interface is mostly unused. Pipelines live in one
class-indexed array rather than adding renderer fields per specialization.

On native backends the class pipelines compile sequentially on a worker, following
MyGO's nonblocking Metal warmup. A frame whose class is not ready uses the general
pipeline and records no specialized draw for it; pixels are identical. WebAssembly
builds the same static pipelines synchronously without assuming browser threads.
An ignored GPU regression forcibly holds every class pending and compares that
fallback byte-for-byte with `NoClasses`.

Reproduction:

```sh
cargo test --locked -p plico-wgpu --test classes -- --ignored --test-threads=1
cargo test --locked -p plico-wgpu --lib \
  unavailable_class_pipelines_fall_back_to_identical_general_pixels \
  -- --ignored --test-threads=1
PLICO_GPU_TIMER=1 cargo test --locked -p plico-wgpu --test classes \
  profile_glyph_and_image_class_gpu_cost -- --ignored --test-threads=1 --nocapture
cargo test --locked -p plico-wgpu \
  --test gradients --test render --test scene --test shadows \
  --test inset_shadows --test continuous_shadows --test batching --test join \
  -- --ignored --test-threads=1
PLICO_GPU_TIMER=1 PLICO_FRAME_BUDGET=1 target/release/gallery \
  --scenario apps/gallery/scenarios/controls.json \
  --capture /tmp/controls.png --frames 10
# Add --general-classes for the same-binary NoClasses comparison.
```

On 2026-11-12, Linux, Intel ARL integrated graphics and Mesa Vulkan, the three
class tests and 45 broader real-adapter regressions passed. Six alternating
release pairs per scene produced byte-identical PNGs for both `controls` and
`effects`. Median prepare/encode times were effectively unchanged: controls
0.25/0.02 ms for both variants; effects 0.25/0.115 ms general versus
0.255/0.12 ms specialized. Submit and GPU measurements varied in opposite
directions between the two scenes, so this is evidence of a smaller selected
shader with unchanged output, not evidence of a wall-time or FPS improvement.
An additional ten alternating pairs after trimming the class vertex interfaces
again produced byte-identical `controls` and `effects` captures. Prepare/encode
were unchanged; submit/GPU medians moved in opposite directions between the two
scenes, so the smaller declared interface is likewise not presented as a measured
frame-time improvement on this adapter.

`ClassShadow` was then measured against `ae2e0a2` (synchronous Plain/Fill, shadows
general). Sixteen alternating one-frame process pairs reduced median wall time
from 140.97 to 125.61 ms after moving all class compilation off startup. Ten
alternating ten-frame pairs measured GPU timestamp medians of 7.50→6.36 ms in
`effects` and 5.52→5.13 ms in shadow-heavy `mines-playing`; wait medians also
fell, while submit time was mixed. All startup, effects, and mines PNGs were
byte-identical. These are local Intel/Mesa results, not cross-adapter guarantees
or CI thresholds. The final path passed three class-oracle tests, the forced
pending fallback test, and 48 broader real-adapter regressions including outer,
inset, continuous-corner, mask, partial-damage and painter-order coverage.

The glyph/image extension was compared with `0184960` (Plain/Fill/Shadow only).
Ten alternating 20-frame process pairs reversed binary order on every pair.
`data-table` and `workspace` GPU medians were unchanged (3.21→3.20 ms and
3.00→3.01 ms); `controls` moved 3.18→3.41 ms. Process medians were effectively
unchanged across the three scenes, and every PNG was byte-identical. The current
renderer nevertheless selected 25–31 glyph and 27–39 image draws per frame in
those fixtures, so the neutral page result is not a failure to exercise the code.

The same-process isolated fill-rate profile removes process startup and alternates
general/specialized frames. Five runs on the same Intel/Mesa adapter measured
image medians of 0.260–0.303 ms general versus 0.210–0.244 ms specialized, and
glyph medians of 0.612–0.773 versus 0.564–0.713 ms. Thus the smaller shaders have
a repeatable isolated GPU benefit, but normal pages remain draw/submit dominated;
this is not presented as a page-level FPS improvement. All 137 integration
hardware tests passed, including byte-exact monochrome, color-emoji and
high-quality rounded-image oracles plus affine sampling, masks, damage, layers,
painter order and native text coverage.

## Painter-order occlusion culling

Plico now follows MyGO's bounded reverse `planCull` before creating image
bindings or isolated-layer targets. A later unmasked, unrounded, borderless,
non-material rectangle with opaque gradient endpoints contributes its interior
core; an earlier draw is omitted only when its complete clip/mask-constrained
visible bounds fit inside one of the eight largest later cores. The one-pixel
core inset excludes antialiased and fractional clip edges. A backdrop-reading
effect or layer clears the accumulated cores because it observes prior pixels.
Effects and layers containing effects remain uncullable, matching MyGO's
renderer-work boundary.
Invalid public quad/image input is still validated even when hidden.

`Renderer::force_no_occlusion_culling(true)` and gallery
`--no-occlusion-culling` reproduce MyGO's `NoCull` comparison. The planner keeps
one reusable vector per isolated-layer depth, and glyph/image-only scenes first
take a candidate fast path without computing bounds. Reproduction:

```sh
cargo test --locked -p plico-wgpu --lib occlusion_tests
cargo test --locked -p plico-wgpu --test occlusion \
  opaque_cover_culls_hidden_work_before_resource_preparation_with_identical_pixels \
  -- --ignored --test-threads=1
PLICO_GPU_TIMER=1 cargo test --locked -p plico-wgpu --test occlusion \
  profile_opaque_panel_stack -- --ignored --test-threads=1 --nocapture
```

On 2026-11-12, Linux, Intel ARL integrated graphics and Mesa Vulkan, five runs of
a synthetic 1920×1080 stack of twelve translucent panels under one opaque page
measured 7.678–11.005 ms with all draws versus 3.114–4.511 ms with culling, a
59–64% reduction in each paired run. The pixel oracle was byte-identical;
separate regressions proved that a hidden image was not uploaded and a hidden
isolated layer allocated no offscreen target. `controls`, `data-table` and
`workspace` each culled zero draws and produced byte-identical captures with the
switch in either position, so this is a navigation/stacked-page and overlay
optimization, not a claim that steady-state gallery pages became faster.

## Border-only ring splitting

Plico also adopts MyGO's `NoRingSplit` experiment for large analytical borders.
A solid border whose fill endpoints are both fully transparent has a rectangular
core where both outer and inner coverage are exactly one, so its shader returns
transparent there. The CPU now represents that source quad as up to four ordered
instances with target-relative hardware scissors around a conservatively inset
core. The scissor is CPU-only batch metadata: the common GPU `Instance` remains
unchanged. Quads with material/shadow content, visible fill alpha, no visible
border, a core below 4096 pixels, or a core below one quarter of the visible quad
stay as one draw. Damage rectangles intersect the split scissors at encoding
time; strips that do not intersect any damage are removed before instance
upload. Isolated layers derive the scissors in the layer target's local
coordinates.

`Renderer::force_no_ring_splitting(true)` and gallery `--no-ring-splitting`
provide a byte-exact A/B oracle. Reproduction:

```sh
cargo test --locked -p plico-wgpu --lib renderer::ring::tests
cargo test --locked -p plico-wgpu --test ring_split -- \
  --ignored --skip profile_large_border_only_panel
PLICO_GPU_TIMER=1 cargo test --release --locked -p plico-wgpu \
  --test ring_split profile_large_border_only_panel -- --ignored --nocapture
```

On 2026-11-12, the same Linux Intel/Mesa Vulkan adapter measured a 1920×1080,
2-pixel rounded border at 5.018 ms unsplit versus 3.809 ms split GPU time
(24.1% lower). Readback-inclusive wall time moved from 37.542 to 36.679 ms
because buffer mapping and CPU pixel copying dominate that test. Straight and
rounded direct draws are byte-identical; a separate moving-border regression
proves partial damage and cropped isolated-layer coordinates against an unsplit
full-redraw oracle. The split path uses four instances/draws instead of one, so
the threshold deliberately excludes small controls. The real
`component-kit-corners` gallery recipe split one large ring (633 instances / 75
draws versus 630 / 72 unsplit) and produced the same PNG SHA-256
`3085f7f0ee2f2e23d84b3d8a0a9c90c21aef3e7242691c3dcd3d5a1b71a7ddef`;
`controls`, `data-table`, `workspace`, `mines-hard` and `patterns-dark` stayed
below the threshold and paid no extra draw calls.

### Rejected: opaque-core splitting on the current Vulkan path

MyGO only enables `addOpaque` for renderers that benefit from writing a large
opaque core without blending; its implementation specifically enables it for
Metal's tile-based GPUs and notes that other renderers would merely add draw
calls for the same pixels. Plico therefore tested this as a backend experiment
rather than adopting it as a general scene transformation.

The experiment split one 1920×1080 solid opaque quad into one no-blend core and
four normally blended antialiased edge strips. A byte-for-byte oracle passed,
but on the Linux Intel/Mesa Vulkan adapter the median GPU time was unchanged:
3.548 ms unsplit and 3.549 ms split. The split also changed one instance and
one draw into five of each and required another pipeline. The implementation
was removed. Do not add opaque-core splitting to the shared wgpu path without
adapter-specific evidence; a future Metal-only experiment should first measure
on real Apple hardware.

### Analytical leaf rounded clips

Plico follows MyGO `92336637` for the first safely reducible clip case: an
axis-aligned rounded clip whose scope contains no nested clip or isolated layer,
whose transformed bounds remain inside its parent rectangle, and which has no
existing coverage mask. Its per-corner radii are retained on `Quad`/`TextRun`
draws and evaluated by the same WGSL coverage function as direct rounded
geometry. Nested/intersecting chains still use the existing mask path.

This removes path rasterization, CPU RGBA mask storage, mask upload and sampling
for common rounded cards. It also fixes the legacy fractional-edge case that
multiplied rectangular clip AA by the raster mask's AA; the analytical result is
byte-identical to drawing the same rounded rectangle directly. Prepared-scene
materialization preserves the radii, rounded clips cannot become rectangular
occlusion covers, and `force_no_analytic_clip_reduction` provides the old mask
path for quality/performance comparisons.

On 2026-11-12, Linux Intel/Mesa Vulkan, a changing 1024×720 rounded clip took
0.306–0.513 ms median to prepare through the mask path and about 0.001 ms
analytically. A static warm clip had no repeatable GPU change (representative
1.681 ms masked versus 1.684 ms analytical), so this is a CPU/memory and edge
quality result, not a GPU-throughput claim. Reproduction:

```sh
cargo test --locked -p plico-wgpu --test analytic_clips -- \
  --ignored --skip profile_leaf_rounded_clip
PLICO_GPU_TIMER=1 cargo test --release --locked -p plico-wgpu \
  --test analytic_clips profile_leaf_rounded_clip -- --ignored --nocapture
```

## Scene and image residency

Prepared snapshots retain only the graph reachable from the current root. Image
cache preflight marks sources across the complete graph, including damage-skipped
subtrees; successful encoding releases textures from removed pages/viewport
variants. Explicitly retained external snapshots still own their resources.
Coverage masks use R8 GPU storage; shared CPU vector/mask raster data stays RGBA.
Image pixel payload statistics therefore count one GPU byte per coverage pixel.

Backdrop preflight reservations and actual draws are separate states. One live
occurrence reserves one cached output, and a later draw may claim that reserved
output. Marking all historical outputs as used before forbidding their reuse
caused linear growth while scrolling. A 96-frame regression checks exact output
counts, bytes and pixels, including two occurrences of one effect ID.

Backdrop input snapshots use a separate exact-specification pool shared by the
single-texture and same-stage array paths. Entries are exclusive within one
frame and reusable in the next ordered queue submission. `LayerPoolStats`
reports their entries, bytes, cumulative allocations and reuses; the ordinary
idle cache trim releases them. GPU regressions cover stable reuse, trim/rebuild
and byte-exact output for both paths.

The paired backdrop mapping and composite uniforms use a second frame arena.
Each draw gets one device-aligned persistent buffer entry; subsequent ordered
frames update it instead of creating 16-byte and 80-byte buffers again. Multiple
damage scissors for one atomic group share its entry. `LayerPoolStats` reports
entry count and cumulative allocation/reuse, and cache trim drops the entries.
This is an allocation-lifecycle result, not a wall-time speedup claim.

These counters describe owned texture payload and caches, not process RSS or
Metal driver allocation. Plicowarden's independent `memory_probe` example records
both cache counters and RSS for synthetic scrolling and resize/page stress.

## 矩形裁剪内部的填充快路径

Plico 参考 MyGO `Builder.markFill`（`ee3d4bd9`），对普通实色填充检查其
抗锯齿 quad 是否完全位于矩形 clip 内，四侧保留 3 个物理像素；满足时不再
执行 fragment 的 clip coverage。靠近边缘、遮罩、渐变、材质、阴影及层标记
保留原路径。专用填充管线与通用管线均识别这个内部标记，不新增 shader
编译或 GPU buffer 布局，也不要求应用修改声明。实例准备时顺带累计
`FrameBatchStats::clip_coverage_skips`，不额外扫描实例流；计数先于 damage
过滤，不等于实际绘制次数。

复现（GPU 测试串行运行）：

```sh
cargo test --locked -p plico-wgpu --lib interior_clip -- --include-ignored --test-threads=1
cargo test --locked --release -p plico-wgpu --lib profile_interior_clip -- --ignored --nocapture --test-threads=1
```

像素对照覆盖两种填充管线、hard/AA clip、fractional clip 和相交边缘，要求
逐字节一致；CPU 断言保证遮罩等不安全情况不会进入快路径。
性能探针在 1536×1024 离屏目标绘制 2,000 个小填充，预热 20 对、采样
100 对，交替开关并反转顺序，等待 GPU 完成但不回读像素。每次优化路径
断言有 2,000 个填充省去 clip 计算。它测的是 CPU 编码、提交和 GPU 等待
的总墙钟，不是 GPU timestamp，也不是桌面 FPS。

2026-11-10 Linux、Intel ARL 核显、Mesa 25.0.7 Vulkan、release 单次配对样本：
专用管线 p50 约 878→877µs，通用管线约 1196→1191µs，变化小于稳定收益
所需的证据，因此只确认减少了冗余计算，不宣称整体性能提升。macOS/Metal
尚未实测；原始日志为本地产物，不提交。
