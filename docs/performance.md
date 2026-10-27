# 性能基准与局部结构同步

## 本轮实现

结构变化不再默认触发全树后端同步。常规 Source 路径使用声明阶段积累的 dirty nodes：
更新变化节点的属性与直接孩子列表，只递归创建尚不存在的子树。共享 visited 集合确保
同一提交里的新增节点不会重复同步，集合容量跨帧复用。

首次挂载和没有 dirty journal 的兼容入口仍执行全树同步。结构提交后仍扫描 retained
节点以清理删除项；布局、flatten、tab order 和呈现树更新也可能遍历全树。
因此这不是整个结构更新过程都变成 O(变化节点数)。

回归测试在 1,000 个静态兄弟节点旁执行：
- 插入含两个节点的子树：同步 3 个节点（父容器及两个新节点）。
- 重排已有孩子：同步 1 个父节点，写一次孩子列表。
- 删除该子树：同步 1 个父节点，移除两个后端节点。
- 显式全树兼容入口仍同步超过 1,000 个节点。

SVG 卸载清理、Tab 顺序、焦点、错误声明后的恢复保留原有行为，复用现有回归测试验证。

## 服务快照

HostService 增加可选的 snapshot_revision() -> Option<u64>，默认 None。
ServiceView 的静态调用路径在版本相同时复用上一份 typed snapshot，不要求 Snapshot: Clone。
服务负责在展示数据变化时改变版本，包括 send 中同步改变的数据；poll 的返回值继续负责
通知宿主是否需要重新声明。版本必须覆盖所有影响 snapshot 的修改。

None 保持每次声明/键盘处理时重新生成快照的行为。快照不进入 UI checkpoint，消息仍在
每次调用后排空；未处理的键盘事件丢弃本次消息。任务示例已提供版本号。

此优化目前用于直接链接的 ServiceView。动态模块路径仍传输 JSON 快照，仍会重建模块侧
Engine；未把模块实例跨调用保留，也未改变现有 ABI 或动态库寿命规则。
大数据快照还可以在服务实现中使用共享存储，但此次没有全局替换数据容器。

## 可重建 CPU / 分配探针

执行：

```sh
cargo run --release -p plico-vello --example performance
cargo test -p plico-vello --test structural_sync
cargo test -p services-demo --test native_services
```

探针使用实际 render_retained_frame 路径，包含声明、后端同步和 CPU 场景编码；
无窗口、GPU 提交和显示刷新，也不导出整份 owned PlacedNode 数组。
每种场景先预热 20 帧，再统计 200 帧。每帧显式请求重建，
“无变化”并非完全空闲窗口。分配统计使用进程级 allocator，
统计分配/realloc 请求次数与请求字节总和，不是存活内存或峰值内存。

2026-10-20 本机 release 单次运行，1,000 个固定文字节点与一个动态容器：

| 场景 | P50 | P95 | P99 | 分配次数/帧 | 请求字节/帧 | 后端同步节点/帧 |
|---|---:|---:|---:|---:|---:|---:|
| 无变化重建 | 171µs | 175µs | 182µs | 0 | 0 | 0 |
| 交替插入/删除一行 | 994µs | 1020µs | 1043µs | 2225 | 3567056 | 1.5 |

时间受机器负载与构建环境影响，不作为 CI 硬阈值。这是修改后的绝对值，未宣称时间上的
前后加速比例。测试约束的是同步访问范围；探针同时断言该路径不克隆 PlacedNode 数组。
原始日志属于本地产物，不参与提交。

下一步值得分析结构更新时的布局输出、flatten 与场景重建分配。
上述插入/删除虽已局部同步，但整体帧成本依然明显高于无变化重建。

## 第二轮：复用 flatten 输出

新增保留容量的 compute_and_walk_with_scroll_into / flatten_with_scroll_into。
Plico 的布局、smooth scroll、滚动钳位、焦点滚动和 popover 重定位使用这些入口，
不再每次从空 Vec 开始构建 PlacedNode 数组。旧的返回 Vec 接口保留兼容。
元素仍会更新并克隆 Paint；没有缓存可能过期的绘制数据。

孩子已经按 (overlay_plane, z_index) 排序时直接遍历，无需临时 Vec 与排序 scratch。
乱序时仍使用稳定排序，保持相同 z 值的声明顺序。单元测试覆盖两条分支、
兄弟重排、孩子删除、Paint 更新和输出缓冲指针/容量不变。

同一探针第二轮修改前后，本机单次 release 运行的结构增删分配：

| 指标 | 修改前 | 修改后 |
|---|---:|---:|
| 分配/realloc 请求次数 / 帧 | 2225 | 2204 |
| 请求字节 / 帧 | 3567056 | 2299896 |

请求字节减少约 35.5%。次数下降较少，说明还有大量小分配，不能用这项改动宣称
已经解决结构更新成本。无变化重建仍为 0 分配。单次时延存在噪声，此处不声称 CPU 加速。

探针增加声明、同步、布局/flatten 和场景编码的平均耗时。布局计时沿用 BackendStats
的范围，还包含滚动/控件几何等后处理，并非纯 Taffy compute。分配计数目前仍为全帧
统计，没有把每阶段的分配请求单独归因。实际 GPU 时间不在测量范围内。

## 静态图片声明的重复编码

`ImageAsset` 保留懒初始化的十六进制编码，复用资源时不再每帧遍历 PNG/JPEG 字节重新编码。
动态字节使用 `Arc<[u8]>`，已初始化的编码使用 `Arc<str>`，克隆资源不复制这两份负载。
保留原 `encoded() -> String` 接口，增加 `encoded_str() -> &str`；前者仍会复制缓存字符串，
因此这项修改没有把图片声明变成纯 ID 操作。资源应由宿主或应用长期保留，不能在每帧重建。
缓存初始化不影响图片的相等性或哈希值；静态和动态来源均按字节内容比较。

runtime 图片解码缓存改为先借用字符串查询，命中时不再构造新的 `Arc<str>` key。
内容变化仍通过原有声明通道触发更新，不改变热重载协议或渲染器接口。
代价是每个已使用资源额外保留约两倍编码字节数的十六进制缓存。

Lyrune 的 `lyric_work` 探针，1280×850、dev opt-level=1、1200 帧、60 帧预热，
相同模拟时钟与图片，关闭 tracing 录制。本地临时覆盖 Git 依赖，对比修改前后的共享层：

| 单帧 CPU 指标，中位数 | 修改前 | 修改后 |
|---|---:|---:|
| 带封面与背景的构建 | 849.6µs | 117.1µs |
| 其中应用声明 | 777µs | 46µs |
| CPU scene 生成 | 32.9µs | 33.5µs |
| 单张图片 `encoded()` | 472.6µs | 7.5µs |

封面为 265508 字节，模糊背景为 14661 字节；构建 p95 从 877.2µs 降到 139.9µs。
无图片对照组构建中位数为 73.0µs / 75.2µs，未出现同量级改善。
该结果定位的是编码开销，不包含 GPU、音频、窗口调度，也不表示整个进程 CPU 降低 86%。
原始 trace、图片和 JSON 均为本地产物，不参与提交。

富文本颜色动画另有一个独立的失效问题：同一组范围、字重、斜体只改变颜色时，
core 不再标记重新测量，runtime 比较文字 run 的度量属性时也忽略笔刷颜色。
增删范围、字号或字重变化仍重新测量。回归测试同时验证绘制指令确实改变、
矩形不变且 Taffy 不执行布局，避免用“没有更新画面”换取更低开销。

Lyrune 同时将歌词位移从逐行绝对位置改为共同容器的 translate；
测试验证持续位移和强调颜色过渡期间的非结构采样不重新布局。
非歌词页的真实播放由宿主进度快照唤醒，不再额外请求显示器频率的应用帧。
同一 CPU 探针在这组修改后，带图构建中位数为 74.0µs，p95 为 94.2µs；
这些数字仍不包含窗口呈现、GPU 或音频线程。

## Damage 覆盖率测量（Phase 0）

`FrameStats::damage_coverage` 记录上次成功绘制中实际重绘的目标 image 比例，
不是均值，也不包含失败的提交。Skia 提供该指标；不支持的后端为 None，HUD
显示 n/a。`PLICO_PERFORMANCE_HUD=1` 在既有帧耗时下方显示 Damage。
trace 的 `plico::perf / damage_coverage` 使用同一已应用区域。

旧的 `Adapter::damage()` 与累计 partial 估计已删除：它依赖节点 dirty 队列，
不完整覆盖旧 ink、阴影和 overflow；保留两套同名统计容易造成误解。
布局运行、flatten 次数继续保留为独立成本指标，不再冒充 redraw damage。
HUD 本身会改 scene，测量正式 CPU 收益时关闭 HUD，使用 trace/外部采样。

## 可选的局部绘制能力

`plico_renderer::DamageRegion` 使用物理像素描述 Full / Empty / Rect，
不依赖 Taffy。后端可以随时提升为 Full，也可以忽略该提示。
LVGL 可继续使用自身布局与失效系统；Bevy 后端可使用自身渲染调度。
布局、damage 生产、保留表面绘制、swapchain 历史和平台增量提交分别接入，
不要求每个后端实现整条管线。

`SkiaSurface::draw_scene_damaged()` 是保留表面的显式实验入口：
矩形向外对齐像素，在外层 clip 内清除并重放完整 scene，保留顺序、重叠和
透明混合语义。目前仍遍历全部命令，不宣称减少 CPU scene 构建成本。
首帧、此前失败、背景改变时回退全量；非有限矩形回退全量。
调用者负责包含旧位置、新位置和全部效果的 ink bounds。
像素测试比较移动、重叠透明层的局部绘制与全量绘制，并验证区域外像素不变。

### 自动 damage 与 Skia 窗口

`SceneDamageTracker` 直接比较完整绘制指令流，和 Taffy/节点身份无关。
它同时合并新、旧和已删除指令的 ink bounds，处理 clip/layer 范围变化；
静态指令及 clip 不变时复用 ink bounds。tracked 绘制跳过脏区外的 leaf 命令，
并可跳过完全不相交的 clip/layer scope；clip/pop 顺序保持完整。仍然线性比较
命令，不宣称 O(Δ) 构建。未知 ink 使用 enclosing clip，
无 clip 则回退 full。过滤器、自定义 shader、特殊混合模式回退 full。
Skia 使用实际字体的 glyph bounds；阴影和 SVG 暂用上述保守回退。
damage 计算不依赖 Adapter 的节点 dirty 队列。

`draw_scene_tracked()` 已接入 Skia 窗口。每张 swapchain image 单独保存最后
成功绘制的 scene 与 bounds，按本次 acquire 的 image 比较，而不是按上一
显示帧比较。resize/recreate 丢弃旧目标，绘制/提交失败后重建；首次绘制与
背景变化全量绘制。Vulkan 队列同步仍走原来的路径。
这解决的是保留 surface 的局部绘制；系统 incremental-present 提示另行按需开启。

- `PLICO_SKIA_DAMAGE=1`：显式开启实验自动 damage；默认关闭。`0` 供全量对照。
- `PLICO_SKIA_DAMAGE_VALIDATE=1`：昂贵的窗口诊断，在同一张 GPU image 上
  比较局部绘制与全量绘制像素；不一致报错，默认关闭。
- `plico::perf` trace 的 `damage_coverage` 是实际应用到目标 image 的区域比例，
  可能比相邻显示帧的 damage 大；这是已应用区域，不是节点 dirty 的推测。

回归覆盖多目标乱序获取、移动、删除、透明层、文字小数位移/变色、stroke、
背景切换、失败恢复，以及 dense→sparse 的恢复。
本地运行 `cargo run -p plico-skia --features window --example damage_window`：
180 帧，含两次 resize；每帧都与同一 acquired image 的全量 GPU 重绘比较。
它是手动探针，不增加 CI 的窗口测试或 GPU 账单。

### 稀疏与密集更新的对照

`cargo run -p plico-skia --features raster --example damage_compare`：
1280×720、2000 条 fill 指令、40 帧预热后采样 120 帧，交替测试顺序。
scene 构建不计时，计时内不读像素；最终像素必须一致。
本机 dev（优化级别按 workspace）一次结果：

| 场景 | 全量 p50 | tracked p50 | 平均实际覆盖率 |
| --- | ---: | ---: | ---: |
| 每帧改变一个局部 | 696.8µs | 309.3µs | 0.19% |
| 每帧改变全部图元 | 691.7µs | 696.0µs | 100% |

密集更新比较本身有成本；未加回退时测得约 715µs→882µs。
当前在矩形覆盖超过 80% 时提升 full，随后 8 次绘制跳过比较，再探测一次；
这是可调整的启发式，不影响像素正确性。探测时若还没有有效历史，要先全量
建立历史，之后再判断覆盖率。测试保证切回稀疏后会重新进入局部绘制。
这些数字只说明 CPU raster 的这一组负载，不代表 GPU/窗口或真实应用收益。
Vello 的子区域能力与平台 incremental present 继续作为独立阶段验证。

### 跳过脏区外 CPU 绘制命令

加入 command/scope culling 后，同一 2000-fill 稀疏 raster 探针一次结果为
696.8µs→84.5µs p50（此前只有 outer clip 时 tracked 约 309µs）。
密集更新为 702.7µs→707.3µs；仍使用上述全量回退。
新增嵌套 clip/layer、剪裁阴影的像素回归，与全量重绘一致。

`cargo run --release -p plico-vello --no-default-features --features skia
--example local-animation` 展示三条进度条和三个 spinner。
正常 UI 使用注入时钟；探针的 FrameSource 通过 deadline 固定约 60Hz，
防止 Xvfb 的 FIFO 不限速影响 A/B。18 秒后退出并打印实际声明帧数；
先确认实际 FPS，再解释 CPU，不能把隐藏/未更新的窗口当成优化结果。

本机 Xvfb + NVIDIA Vulkan 的一次 8 秒采样（预热 3 秒）：

| 模式 | 单核归一化进程 CPU | 18 秒声明帧数 |
| --- | ---: | ---: |
| 全量 | 10.0% | 1059 |
| 自动 damage + culling | 9.625% | 1060 |

通过 `/proc/PID/stat` 的 utime+stime 差与 `getconf CLK_TCK` 计量，包含进程线程。
这是小动画场景：没有证明单核低于 1%；也没有证明 Lyrune 达到该目标。
本机 24 逻辑核，按所有逻辑核归一化约 0.4%，应与单核百分比明确区分。
Xvfb 报告缺少 DRI3，呈现路径和真实桌面不同；真实窗口的收益仍需在持续
显示、帧率稳定的条件下测量。测量脚本和日志是本地产物，不增加 CI 窗口测试。

### Vulkan incremental present（可选）

`PLICO_SKIA_INCREMENTAL_PRESENT=1` 开启系统区域提示，默认关闭，避免未测量
收益就增加逐帧比较。创建窗口设备时查询并启用 `VK_KHR_incremental_present`；
不支持时仍按原路径提交。当前 surface transform 非 identity 时也不发送提示，
避免把 image 坐标误当成旋转后的 surface 坐标。

每张 image 的修复区域与系统提示使用两份历史。系统提示单独比较上次呈现的
scene：A→B→A 时，buffer A 本身可能无需绘制，但屏幕仍有 B→A 的变化。
背景变化、resize/recreate、绘制失败、无有效历史，以及目标需全量绘制时，
提示回退 full。Full/Empty 都省略可选 pNext；Vulkan 的零个 rectangle 表示
full 而非 empty。实际提交只附带非空、向外取整并裁到 surface 内的矩形。
所有像素仍保持完整有效，系统忽略提示也能正确显示。

约束参考 [VkPresentRegionKHR](https://docs.vulkan.org/refpages/latest/refpages/source/VkPresentRegionKHR.html)
和 [VkPresentRegionsKHR](https://docs.vulkan.org/refpages/latest/refpages/source/VkPresentRegionsKHR.html)。
窗口在 queue-present 前调用 Winit `pre_present_notify()`，供 Wayland 帧回调调度。

本机 NVIDIA/Xvfb 探针：扩展可用，180 帧中 168 次局部绘制、164 次局部提交
提示，所有 framebuffer 像素与全量重绘一致。这验证提交路径和区域计算，
不证明 compositor 采用提示或节省了多少 CPU/GPU。没有安装 Khronos validation
layer，不能把该测试当作 Vulkan validation layer 检查。
`PLICO_SKIA_INCREMENTAL_PRESENT=0` 可在同一探针中验证无提示回退路径。
trace 的 `present_damage_coverage` 是相邻呈现 scene 的估计覆盖率，与目标
buffer 的 `damage_coverage` 不同；Empty 在平台接口仍退回普通提交。

### Vello 接入难度评估（不改窗口）

本机 `vello_gpu 0.3.0` 已公开 `ClearSettings::Rects`，其 root clear pass 使用
Load 保留其他像素。需要同时给 scene 加外层 clip；仅局部 clear 后重放完整
scene 会在脏区外重复透明合成，不能直接替换现在的全量 clear。

手动 GPU 探针：

```sh
cargo test -p plico-shell --lib vello_preserved_target_partial_redraw_probe -- --ignored --nocapture --test-threads=1
```

在稳定的离屏纹理上跑 60 帧，覆盖矩形移动、移除、透明 layer、背景变透明、
以及目标尺寸重建；局部清除 + clip 的像素逐帧与全量重绘一致。
此测试默认 ignored，不增加 CI GPU 运行；正常 ImageRenderer 和窗口代码未变。
它验证 Vello API 的可行性，不证明字体、SVG、滤镜和自定义 shader 的完整性，
也没有给出性能收益。

窗口路线可复用 `wgpu_context 0.10` 的 intermediate texture：
`target_texture_view()` 在配置中间纹理时返回稳定 view，
`maybe_blit_and_present()` 会把它复制到本次获取的 surface texture；resize
会重建该纹理。Plico 目前主要在 alpha 转换需要时才启用中间纹理，不能直接
假定普通窗口的 surface texture 在下一帧仍保留内容。

因此基础接口改动不大，完整窗口接入为中等工作量：始终使用保留目标、首帧/
resize/错误回退、复用共享 damage 协议，补 Vello glyph ink 的保守范围，再
验证 layer/filter、透明窗口、不同 DPI 和设备丢失。全屏 blit 仍有带宽成本；
必须用真实负载测量，不能只看局部绘制面积。

另一个独立观察：当前 Vello 窗口配置为 `PresentMode::AutoNoVsync`；后续比较
动画 CPU 时应核实实际 FPS，并对照同步模式，避免将不同帧率误判为 renderer
差距。本轮没有修改它，也没有启用 Vello 自动 damage。


### 实验默认值与已知差异

Skia 窗口默认全量绘制。设置 `PLICO_SKIA_DAMAGE=1` 才启用自动 damage；
`draw_scene_tracked()` 仍是显式实验 API。Vello 的 capability probe 继续保留，
没有删除或接管其正常窗口路径。

真实 settings 的虚拟时钟主题动画在第 96 帧发现 4 个像素差异，每通道最多
1（8-bit）。关闭 CPU culling、仅使用相同 outer clip 也能复现同样的 4 像素；
将 clip 向外扩 4 像素后该帧一致。这将范围缩小到 clip 附近的栅格化/抗锯齿
差异，尚未证明所有场景可由固定 padding 正确解决，因此保持 opt-in。

`cargo run -p backend-settings --features skia --example damage_probe` 使用真实
设置页、语义点击和虚拟时钟跑 240 帧，覆盖侧栏、导航、主题、文字修改及静止。
本机 raster 结果中 121 帧有 AA 舍入差异，最多 8 个像素、每通道 1/255；
静止阶段未继续累积。该手动探针明确允许最多 16 个这样的像素，并报告实际数量；
越界立即失败。除此之外，damage 区域外与上一帧必须逐字节相同，Empty 帧必须
完全保留，均不使用容差。设置 `PLICO_DAMAGE_STRICT=1` 可恢复严格全帧逐像素
比较，当前会在第 96 帧失败。独立透明图层最小复现保留为 ignored 诊断测试，
常规 raster/GPU 回归仍使用精确相等。以上是该场景的验收范围，不代表所有
栅格差异都可以忽略，也没有改变窗口 strict validation 的行为。


### 分阶段验收状态

- Phase 0：实际目标覆盖率已接 HUD/trace，设置页探针记录真实交互覆盖率；
  动画探针及 Lyrune 对照记录 CPU，明确区分窗口与离屏测量。
- Phase 1：共享 DamageRegion、独立 command tracker、Skia raster 保留绘制和
  命令剔除已接入；常规回归严格比较，真实设置页另有明确 AA 容差诊断。
- Phase 2：Skia swapchain 每张 image 保存独立历史，首次/resize/错误恢复回退；
  180 帧窗口 oracle 含两次 resize，逐帧和全量 GPU 绘制一致。
- Phase 3：Vello 保留目标子区 clear + clip 的离屏验证已完成；按当前范围，
  不启用其窗口局部绘制，后续按需要独立接入。
- Phase 4：Vulkan 增量提交提示已接入，支持探测及无扩展/关闭时回退；
  同一窗口 oracle 在开启/关闭提示时分别通过（164 次/0 次提示）。

上述能力全部可选，普通窗口默认全量绘制。已知透明图层 AA 差异不阻塞该实验
入口；严格诊断仍可复现它。原生 Lyrune 的进一步 CPU 收益尚未得到证明，
不以离屏收益替代原生窗口结论。下一轮应由真实负载决定是否扩展 Vello 窗口
或优化 damage 比较成本，而不是继续为微小舍入差异改变渲染架构。


### Skia 字体集合内存

`new_from_bytes` 会复制输入的完整字体文件；对 TTC 的每个 face 分别调用会
重复保留整个集合。damage 的 glyph ink 查询也会创建 Typeface，放大该成本。
修复前真实 Lyrune 进程中可识别 TTC 头的匿名映射约 547 MiB，同一字体头
出现多块副本；这不是 glyph atlas 大小的测量。

Skia 资源池现在按 Blob ID 保存一份 owned `SkData`，不同 TTC index 使用
`new_from_data` 共享它；源 Blob 已失效时清理条目，Empty 绘制也执行清理。
不通过无生命周期保证的裸字节借用实现零拷贝。常规测试验证跨字号复用与
失效清理，手动 TTC 测试验证集合内多个 face 只有一份资源池数据：

```sh
PLICO_TEST_TTC=/path/to/font.ttc cargo test -p plico-skia --features raster --lib collection_faces_share_one_font_data_allocation -- --ignored
```

`plico::perf` 的 Skia target trace 另外记录 `gpu_cache_bytes`、
`gpu_cache_purgeable_bytes`、`gpu_cache_limit_bytes`。这些是 Ganesh 缓存记账，
不包含字体文件副本、驱动分配或进程全部 RSS，不应混作进程内存。

修复后本机 Xvfb/NVIDIA 上的两个独立 Lyrune 主界面进程，5/15/30/45 秒采样：
全量 RSS 约 378 MiB、匿名约 163 MiB；damage RSS 约 336 MiB、匿名约 158 MiB，
15–45 秒未继续增长。这是启动主界面测试，不是长时间歌词播放验收；
实际播放中的旧进程需重启后再对照，不能拿不同页面/播放时长直接计算节省比例。


### Fontique 来源缓存淘汰后的重复加载

仅共享 Skia TTC 数据还不够：Parley 每次创建 layout 会推进 SourceCache 的
淘汰代数；默认非共享缓存丢弃条目后，再次访问同一字体会创建新的 Blob ID。
旧布局/glyph scene 仍可能持有之前的 Blob，Skia 因而也无法按 ID 复用资源。
`TextContext::new()` 现在调用 `font_cx.source_cache.make_shared()`；它的次级
weak 缓存可找到仍被布局持有的 Blob，不要求让来源缓存永久强持有所有字体。
这是 Plico 保留布局与来源缓存配置的配合问题，不意味着禁用 Parley 的淘汰。

回归先在旧配置下复现字体 ID 改变，修复后通过；另连续排版 600 个不同文本，
验证自然缓存老化后 ID 仍保持不变。114 项 renderer 单元测试通过。

Lyrune 的 `lyric_damage` 手动探针增加 RSS、累计字体 Blob ID 与 Ganesh 缓存
采样；`--font-cache-stress` 每 600 帧主动淘汰来源条目，
`--unshared-font-cache` 只用于恢复旧配置做对照。相同 10,000 帧、1.75 倍缩放、
Intel Vulkan、真实封面及歌词逐字高亮/滚动的一次离屏结果：

| 配置 | 累计字体 Blob ID | RSS 峰值 | 最终 RSS |
| --- | ---: | ---: | ---: |
| 旧非共享来源缓存 | 34 | 1053 MiB | 1017 MiB |
| 共享来源缓存 | 2 | 225 MiB | 225 MiB |

这是主动加速缓存淘汰的压力实验，不是用户原生窗口内存的直接测量。
未强制淘汰的 10,000 帧对照同样维持 2 个字体 ID，RSS 约 225 MiB。
原生窗口仍须重启并在实际播放中观察，不能将旧进程的保留副本视为新版结果。

## Flatten 阶段 profiling

`profiling` feature 增加五个 `plico::layout` trace span：
`layout.compute`（Taffy）、`layout.flatten`、`backend.index`、
`backend.presentation`、`backend.selection`。不启用该 feature 时不创建这些 span。
现有 `BackendStats::layout_micros` 是后端更新的宽泛计时，包含这些工作及其他
步骤，不能直接当成 Taffy 或 flatten 时间，也不能与内部 span 相加。

```sh
cargo run --release -p plico-vello --features profiling --example flatten_profile
```

Rust 探针用虚拟时钟驱动静态、重新声明、单点颜色、透明度、位移、尺寸、
非虚拟滚动、虚拟滚动和结构更新，并包含真实 gallery 页面。输出 CSV。
合成场景预热 40 帧、采样 120 帧；gallery 预热 60 帧、采样 120 帧。
只订阅上述阶段，不记录每个节点的 span；数据是带少量 tracing 开销的阶段
wall-time，不是硬件采样。帧总时间不包括输入事件处理、GPU 或窗口呈现。
静态结果表示尝试构建一帧的成本，不意味着窗口空闲时应持续请求帧。

本机 release 的代表性结果（微秒，值随负载/缓存状态波动）：

| 场景 | 实际 placed 节点 | 帧构建 p50 | flatten p50 | flatten 累计占比 |
| --- | ---: | ---: | ---: | ---: |
| gallery Workspace idle | 92 | 0.17 | 0 | 0% |
| gallery Layout 连续 resize | 96 | 105.8 | 7.4 | 7.0% |
| gallery Motion 周期展开 | 78 | 33.2 | 0（多数帧不执行） | 3.8% |
| 200 行非虚拟滚动 | 402 | 172.4 | 30.1 | 17.5% |
| 2,000 行非虚拟滚动 | 4,002 | 1968.4 | 423.2 | 21.9% |
| 10,000 行非虚拟滚动 | 20,002 | 20299.2 | 5163.7 | 25.3% |
| 10,000 行虚拟滚动 | 55 | 54.7 | 4.9 | 8.8% |

无变化的重新声明、单点颜色、opacity 和 translate 场景的 flatten 调用均为 0。
对照不是说所有动画都不需要布局：尺寸与结构变化仍调用 Taffy 和 flatten。
虚拟列表窗口切换也会重建可见部分，但实际节点数保持在几十个。

另在计时循环外，对同一 placed 快照测量 `clear + extend_from_slice`：
402/4,002/20,002 个节点的热缓冲克隆 p50 约 20/222/1125µs。
这是复制代价的独立下界对照，不可用两组中位数之差断言“纯遍历”时间。
现有增量 flatten 草稿仍检查节点并克隆整个复用块，不能据此宣称 O(1) 跳过。

Lyrune 的 `lyric_damage` 也增加 `lyric.build` 根 span，并复用 tracing 初始化。
可在该仓库使用以下命令（封面自备，报告仍留在忽略目录）：

```sh
RUST_LOG=warn PLICO_TRACE_FILTER='plico::layout=trace,plico::perf=trace' PLICO_TRACE_FILE=reports/plico/flatten-trace.json cargo run --release -p lyrune-plico --features skia,profiling --example lyric_damage -- --cover cover.jpg --mode damage --frames 600 --scale 1.75 --device Intel
```

SkParagraph、真实封面、歌词滚动/逐字高亮，两次运行排除 60 帧预热后：
600 帧仅 25 帧调用 flatten；执行帧 p50 13.2–16.9µs，累计占 CPU 帧构建
0.45–0.49%，帧构建 p50 128–132µs。这里不包括 GPU 提交；没有证明原生窗口
总 CPU 的同等占比。即使完全移除这部分，也只消除该构建负载不到 0.5%。

更有价值的下一项是收窄选区刷新：合成 4,002 节点单点颜色更新，flatten 为零，
`backend.selection` 平均仍约 449µs；20,002 节点时约 3970µs。
这说明不能把宽泛后端更新时间归因于 flatten。当前建议先使用虚拟列表、
按依赖限制选区/其他后端扫描，再按实测决定是否需要保留块式 flatten。
本轮只增加测量与文档，不启用或提交既有未接入的增量 flatten 草稿。

## Lyrune 歌词页面：Skia 阶段测量

SkParagraph + GPU distance-field，实际歌词 UI、逐字高亮、平滑滚动和 JPEG 封面，
1280×850 logical、1.75× device scale、Intel ARL GPU。虚拟播放时钟以 60Hz
推进，预热 60 帧。歌词为 demo fixture；没有账号网络、音频、窗口合成器或
swapchain，不能把结果当作完整播放器 CPU 或输入延迟。

关闭 trace，600 帧按 full → damage → damage → full 顺序分别测量：

| 模式 | CPU（全部 24 个逻辑核） | 构建 p50 | 绘制与提交 p50 |
| --- | ---: | ---: | ---: |
| 全量，两次 | 0.658–0.662% | 299–303µs | 2338–2350µs |
| damage，两次 | 0.450–0.471% | 343–359µs | 1580–1611µs |

CPU 是进程 user+system 时间 / 墙钟时间 / 24，相对降低约 29–32%。
CPU 包含驱动线程，提交 wall-time 不能当成 GPU 执行时间。顺序运行仍会受
频率与其他进程影响。damage 比较有成本，但构建中位数之差不能解释为 damage
阶段耗时。各组结束 RSS 约 150–152MiB，不证明长时间播放不存在泄漏。

另以 1200 帧记录 Chrome trace，damage 平均覆盖 28.2%，1179 帧局部更新。
trace 有额外开销，以下仅用于阶段定位，不与无 trace 绝对数混用：

| 阶段 | 每帧平均 wall-time | 说明 |
| --- | ---: | --- |
| Skia flush + submit | 988µs | 不含显式 GPU 同步读回 |
| Skia replay（扣除递归 replay 与段落 miss） | 833µs | 指令回放及 Skia CPU 工作 |
| 完整 UI/后端构建 | 506µs | 包含声明、布局、选区等 |
| 两个封面控件声明 | 164µs | 在声明内部，不能再次累加 |
| 段落缓存 miss | 56µs | 共 1362 次，发生在构建或回放内 |
| 选区刷新 | 38µs | 每帧执行 |
| damage 比较 | 33µs | 每帧执行 |
| flatten | 2.1µs | 共 56 次，并非每帧运行 |

嵌套 span 的 inclusive totals 不能直接求和。图片声明源码仍调用
`Cow::Owned(asset.encoded())`：虽不再重复 hex 编码，却复制整个缓存字符串。
这是优先验证的可消除成本。段落 request 包含颜色，逐字高亮可能产生新 request；
不能由 miss 数断言每个字符都重新排版，或颜色是全部 miss 的来源。

建议顺序：先去掉图片声明的重复复制并复测；再拆分 replay/flush 内部的
图片、文字和图层成本；随后验证颜色变化能否复用整形、选区刷新能否跳过。
当前数据不支持优先投入增量 flatten 或常驻响应式图。

`plico-skia/profiling` 增加 `plico::skia` spans：`skia.damage`、`skia.replay`、
`skia.paragraph_miss`、`skia.flush_submit`。shell profiling 仅在 Skia 已启用时
传播该 feature。原生窗口另有 `skia.wait_fence`、`skia.acquire`、
`skia.queue_present`；等待不等于 CPU 忙碌。不启用 profiling 时不编译这些 span。

在 Lyrune checkout 中复现：

```sh
cargo build --release -p lyrune-plico --features skia,profiling --example lyric_damage
RUST_LOG=warn PLICO_TRACE_FILTER='plico::layout=trace,plico::perf=trace,plico::skia=trace,wabou::perf=trace' PLICO_TRACE_FILE=reports/plico/lyrics-trace.json target/release/examples/lyric_damage --cover cover.jpg --mode damage --frames 1200 --scale 1.75 --device Intel --paced
```

省略 `PLICO_TRACE_FILE` 测无 trace 基线，`--mode full` 对照全量绘制。
`--no-highlights`、`--no-scroll`、`--paused` 分别隔离逐字高亮、平滑滚动和播放。
paused 仍强制调用帧入口，不能据此推断窗口空闲时的唤醒频率。
`--verify` 用于正确性检查，性能采样不要开启。本机 Xvfb 缺 DRI3，窗口试跑
不作为实际桌面 present 性能证据。报告保留在忽略目录，不参与提交。

同设备、同 scale，关闭 trace 的 600 帧功能对照（各一次，非统计显著性结论）：

| 场景，damage 模式 | 全机 CPU | 构建 p50 | 绘制提交 p50 | 平均 damage 覆盖 |
| --- | ---: | ---: | ---: | ---: |
| 关闭逐字高亮，保留平滑滚动 | 0.329% | 306µs | 983µs | 4.5% |
| 关闭平滑滚动，保留逐字高亮 | 0.546% | 461µs | 1654µs | 24.1% |
| 暂停，仍强制 60Hz 调用 | 0.0083% | 3.4µs | 27.5µs | 0%，600 帧 Empty |

关闭高亮同时改变文本样式更新和重绘范围，不能把全部收益归因于整形。
关闭平滑滚动改为跳转，不是从原场景简单减去一个动画；本轮反而更贵，
需单独追踪布局/跳转后才能解释。暂停结果说明保留缓存有效，不说明真实窗口
仍需要 60Hz 唤醒。后续优先拆分逐字高亮的段落缓存与 damage 范围成本。

