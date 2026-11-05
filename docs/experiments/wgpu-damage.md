# wgpu damage 收益验证

2026-11-05。结论：局部动画能显著减少 GPU 工作；窗口进程 CPU 不一定同步下降。
初版大面积滚动会增加 CPU 编码成本；随后参考 MyGO 加入 >=50% 面积全量回退，
复测滚动 CPU 回到全量水平，现已默认开启窗口 damage，`PLICO_WGPU_DAMAGE=0` 可关闭。
下文先保留回退前基线，最后一节记录当前策略的复测结果。

## 后端受控实验

使用实际 Intel Graphics (ARL) 核显、Mesa 25.0.7、Vulkan，非软件 adapter。
工作站为 Core Ultra 9 285K，Linux、Rust 1.99.1；使用仓库现有优化 dev/test 配置。

手动 ignored test 位于 `crates/plico-wgpu/src/damage_profile.rs`，不作为 CI 性能门槛。
它调用真实 `plan_scene_frame` 和 `encode_frame_regions`，包括与窗口相同的全屏 blit。
测试专用编码入口只在 Linux/system-text 单元测试构建中存在，不增加公开 renderer API。

统一 dashboard 包含 72 张带原生中日文文字的卡片、进度条和三个 spinner。
预先生成相同 Scene 序列，不把业务声明和场景构建计入后端时间。分别测量：重复相同
场景、一个进度条、三个 spinner、整个列表滚动、全屏颜色变化。两档尺寸为
1280×800 和 2560×1600（逻辑内容相同，后者 scale=2）。

每场景每档尺寸先验证 8 帧及删除全部内容的一帧，partial 与 full RGBA **逐像素一致**，
共 90 组对照。像素读回不在性能计时内。计时分四轮，每轮正反交替开关；每组预热
24 帧、采样 96 帧。下表是四轮各自 p50 的中位数，时间单位毫秒：

1280×800：

| 场景 | CPU 编码与提交 off → on | GPU 命令 off → on | on 平均脏区面积 |
| --- | ---: | ---: | ---: |
| idle | 0.288 → 0.008 | 0.704 → 0.000 | 0.00% |
| progress | 0.328 → 0.199 | 0.537 → 0.212 | 0.23% |
| spinners | 0.483 → 0.384 | 0.531 → 0.224 | 0.35% |
| scroll | 0.325 → 0.522 | 0.499 → 0.472 | 74.57% |
| full | 0.318 → 0.323 | 0.388 → 0.381 | 100.00% |

2560×1600：

| 场景 | CPU 编码与提交 off → on | GPU 命令 off → on | on 平均脏区面积 |
| --- | ---: | ---: | ---: |
| idle | 0.345 → 0.007 | 1.146 → 0.000 | 0.00% |
| progress | 0.379 → 0.209 | 1.261 → 0.518 | 0.19% |
| spinners | 0.573 → 0.442 | 1.262 → 0.519 | 0.27% |
| scroll | 0.397 → 0.945 | 1.261 → 1.241 | 73.66% |
| full | 0.380 → 0.392 | 1.265 → 1.257 | 100.00% |

CPU 值包含 Scene 准备、damage 计算、实例编码、全屏 blit 编码与 queue submit；
不包含 GPU 等待或整屏像素读回。GPU 值使用设备 timestamp queries，起止空 compute
pass 与实际 scene/blit command buffer **同一次提交**，避免把 CPU 准备间隙算成 GPU 时间。
计时保留 query 边界的固定开销；每帧只读回 16 字节查询结果并同步等待，因此这是
串行后端延迟实验，不是最大吞吐或完整 input-to-photon 测量。idle/on 没有提交命令，
GPU 记为 0，96/96 帧全部跳过；这是重复帧控制组，正常静态应用本来就不应持续触发 rAF。
没有测量 GPU 功耗，不能把时间下降直接描述为功耗下降比例。

## 为什么滚动退化

scale=2 时，全量绘制约 1055 个实例、171 次 scene draw calls。
进度条 damage 后为 3 个实例、1 次 draw call；三个 spinner 约 19 个实例、57 次 draw calls。
但滚动仍涉及约 1009 个实例，多个脏矩形分别回放，让 draw calls 放大到 **1128 次**。
约 74% 的脏区并没有换来有意义的 GPU 时间节省，CPU 编码反而从约 0.397 ms
增加到 0.945 ms。这证明需要选择 full redraw，而不能只用“存在 partial damage”决定逐矩形回放。
本次已加入 >=50% 覆盖率全量回退；矩形数量/回放代价还可用于后续更细的策略。

局部动画 GPU 时间下降约 58–61%，并未按不到 1% 的脏区面积同比下降：
保留目标最终仍需全屏合成到呈现纹理，存在固定带宽/提交成本。减少这项成本属于
后续合成/present 优化，不应与目前 scene scissor 的收益混算。

## 实际窗口 CPU 控制组

`plico-desktop` 的 `wgpu-damage-window` 示例提供三个进度条、三个 spinner 和静态文字行，
使用正常 immediate Source、Pango 文字、窗口宿主和约 60Hz 的 deadline 调度；18 秒自动退出。
源中用注入时钟驱动动画，真实 Instant 仅用于宿主 pacing。

本机当前 Wayland 会话只给探针首帧，未得到可用的连续帧样本，因此没有记为通过。
改用独立 X11/Xvfb 窗口，日志确认实际 adapter 为 **NVIDIA RTX 5080 / Vulkan**，
不是 llvmpipe。它与上面的 Intel 核显是不同设备，不能把两部分数字当成同一硬件流水线。
该虚拟 X11 测试也不代表真实 Wayland/物理显示器的合成开销。

顺序 off/on/on/off，各启动一次进程。等首帧后预热 3 秒，再采样 10 秒的
`/proc/<pid>/stat` 用户态+内核态 CPU ticks；均正常自动退出，帧数如下。
整机口径为单核口径除以本机 24 个逻辑 CPU，未把 Xvfb/桌面合成器 CPU 算入进程：

| damage | 单核 CPU（两轮中位值） | 整机 CPU（24 核归一化） | 每次进程总帧数（18 秒） |
| --- | ---: | ---: | --- |
| off | 18.20% | 0.758% | 1054, 1054 |
| on | 18.00% | 0.750% | 1054, 1053 |

两组均约 58.5 FPS。窗口进程 CPU 的差异约 1% 相对值，落在两次采样波动范围，
**不能宣称 CPU 获得了显著改善**。后端局部 GPU 工作确实减少，但整个窗口仍有业务声明、
布局/Scene 准备、事件循环和呈现的共同成本；需要另行采样才能确定窗口 CPU 的主导阶段。

## 复测

```sh
cargo test --locked -p plico-wgpu --lib compare_damage_cpu_gpu_and_pixels -- --include-ignored --nocapture

PLICO_WGPU_DAMAGE=0 cargo run --locked -p plico-desktop --example wgpu-damage-window
PLICO_WGPU_DAMAGE=1 cargo run --locked -p plico-desktop --example wgpu-damage-window
```

窗口 CPU 应先 build 再直接执行二进制，排除 Cargo 编译和启动成本；确认 adapter、
实际帧数和采样区间一致。不要用改变刷新率、隐藏窗口或丢失 rAF 后的低 CPU 当成优化收益。
保留测试源码和结论，原始日志/逐轮 CSV/JSON 仅保存在忽略的 reports 目录。

## 默认开启与 MyGO 策略复测

读取本机 MyGO `ee3d4bd9bd741f197de231b9fd9b002e9db55577`：
`ui/gpu_linux.go` 把 GL renderer 的 Partial 设置为 true，`MYGO_PARTIAL=0` 可关闭；
`internal/gpu/damage.go` 的 Part.Set 在脏区面积达到 50% 时返回 false，以全量绘制。
它最多保留八个矩形，Plico 原有 tracker 已有同样的数量上限。来源链接记录在
`crates/plico-wgpu/SOURCES.md`，没有移植其 swapchain age 实现。

Plico 现在默认开启窗口 damage；`PLICO_WGPU_DAMAGE=0` 或 `false` 可关闭。
Scene planner 对 >=50% 覆盖率选择一次 full replay，但不丢弃 tracker 已更新的历史，
所以下一帧仍可恢复小区域更新或跳过相同帧。HUD/coverage 继续报告逻辑脏区面积，
trace 的 full_redraw 字段报告实际全量决策。

同一 Intel 设备、相同协议重跑 90 组严格像素对照及四轮计时；2560×1600 结果：

| 场景 | CPU off → on（ms） | GPU off → on（ms） | scene draw calls off → on |
| --- | ---: | ---: | ---: |
| progress | 0.429 → 0.206 | 1.257 → 0.482 | 171 → 1 |
| spinners | 0.678 → 0.489 | 1.255 → 0.517 | 171 → 57 |
| scroll | 0.477 → 0.475 | 1.251 → 1.253 | 171 → 171 |

滚动的 draw calls 已从原 partial 的 1128 次回到 171 次，CPU 与 full 基本相同；
小区域 GPU 收益保留。另有回归验证“全量回退 → 小区域 → 相同帧”的像素和历史，
以及默认开关的无环境变量/0/false 行为。窗口 CPU 表是回退前的控制组，没有因新默认值
而重复声称得到 CPU 改善。Windows/macOS 物理显示器行为仍需实机验收。

## MyGO 风格的 batch / scissor 调度

继续读取同一 MyGO revision 的 `internal/gpu/gl/gl.go` 的 drawBuilt：批次作为外层循环，
绑定纹理/程序一次，再回放与该批次相交的损伤矩形。Plico 原先把矩形作为外层循环，
每个矩形都重新做 batch 分类并提交全部 batch，即使几何完全不相交。

当前改为先清空互不重叠的 dirty rectangles，再按原画家顺序处理每个 batch，
只向与其保守几何包围盒相交的矩形提交；缓存最后一个 scissor，避免重复状态命令。
包围盒含 shader 顶点阶段的一像素 AA 外扩，自定义 material batch 不做此边界裁剪。
全量路径不计算 batch 包围盒。没有重排节点、改变混合规则或增加长期缓存/生产 API。
该调度依赖 tracker 输出的矩形互不重叠，不能把它直接用于任意重叠矩形集合。

为了避免前后两次启动时 GPU 频率变化造成误判，又增加测试构建专用开关：同进程
交替启用/禁用 batch 包围盒裁剪，两组都启用 damage 且使用新 batch 调度。开关不进入
正常 renderer 构建；此 A/B 隔离的是裁剪本身，不包含外层循环调整的全部收益。
每组仍预热 24 帧、采样 96 帧，正反交替四轮；三 spinner 场景结果如下（ms）：

| scale | draw calls/帧，裁剪 off → on | CPU off → on | GPU off → on |
| --- | ---: | ---: | ---: |
| 1 | 68.25 → 24.75 | 0.3673 → 0.3608 | 0.2779 → 0.2580 |
| 2 | 57.25 → 21.08 | 0.5335 → 0.5265 | 0.7718 → 0.7761 |

无效 draw calls 减少约 63–64%；CPU 中位值只减少约 6–7 微秒（约 1–2%），
不能描述为显著的总 CPU 优化。scale=1 的 GPU 小幅改善，scale=2 基本无变化；
现阶段主要收益是确定减少无效命令，Scene 准备与最终全屏合成成本仍在。
单脏区进度条和走 full fallback 的滚动控制组没有明显时间变化。

90 组 full/partial 逐像素检查通过；另有三处半透明图片的三档 DPI 测试，要求每批仅
提交到自己的脏矩形。图层、裁剪、图片、渐变、effect 和 fragment 的 35 项 GPU 回归通过。

```sh
cargo test --locked -p plico-wgpu --features window --lib compare_batch_culling_cpu_gpu_and_pixels -- --include-ignored --nocapture
```
