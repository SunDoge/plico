# 增量执行

Source 将业务声明、后端同步、布局、flatten 和 Scene 编码分别处理。普通 immediate 应用由最后一次成功提交的 kind、文字 revision 和 style 形成 dirty journal；Regions 另外把局部 replay 产生的真实节点变化送入同一 Adapter。声明失败后的恢复强制完整同步，避免遗漏中间状态。

Adapter 在结构稳定时只同步 journal、活跃动画 owner、hover/pressed/focus 的新旧节点；主题变化保守失效全部节点。paint-only 更新直接 patch 已有 placed nodes，不运行 Taffy layout 或 flatten。

## Reactive 几何与 Scene（2026-10-29）

几何变化仍从 root 调用 Taffy，保留约束求解和 Taffy 自身的干净子树缓存；本轮没有宣称布局求解已经增量。Adapter 缓存每个 retained 节点的上一轮 `taffy::Layout`，从实际 geometry dirty 节点向上检查祖先并检查沿途兄弟，只对布局确实变化的 topmost 子树重新 flatten 和 splice。固定尺寸父节点不能只比较自身 box：改变 `gap`、alignment 或 distribution 可能只移动后代，因此 dirty root 下还会检查全部后代。未放置的零尺寸节点提升到最近的已放置祖先。

局部 replay 输出实际节点变化时递增 Region revision；Source 只失效 revision 变化的 Scene Fragment，卸载时只删除对应 cache entry。插入、删除和 reorder 可以尝试局部 flatten，拓扑 root 即使 Taffy box 不变也参与检查，因为 preorder 可能变化。32 节点以上的树若有多个结构 patch，或 patch 覆盖至少一半节点，则改为全量 flatten，但仍尽量保留独立 Region Scene；大面积 mount/unmount（创建加删除节点数至少占全树四分之一）直接回退全量 geometry 和全量 Scene，避免先支付昂贵的候选分析再放弃局部路径。resize、主题或无法确认安全上下文时也完整 flatten。

Regions 的缓存区域边界作为 Scene fragment 缓存边界。干净区域以 `PaintCommand::Fragment` 引用上一帧 Scene，不再把全部命令逐条复制到根 Scene；脏区域及其缓存祖先重新编码。`each` 列表整体是一个缓存边界，keyed item 默认不再各自建立小 Fragment。缓存键包含父 transform、继承 clip、viewport 和 device scale。backdrop blur 会读取之前的 Scene 内容，因此相关子树保持 inline，不独立缓存。普通全量编码、Skia/Web 消费和正确性 oracle 使用同一命令模型；`Scene::flattened()` 递归展开 fragment 后可逐命令比较。

Seeded Reactive 差分已实际接入 Scene cache roots；它与完整 Settings 的路由/输入/Activity reverse/主题/动画/resize 测试一样，每帧将增量 Scene 展开后与同一 Source、同一 TextContext 的全量 reference 比较。renderer 另有 transform/clip/opacity layer、继承上下文、嵌套 fragment 和 backdrop 回归。固定父容器 `gap`、条件卸载后间距、keyed reverse 与独立 Fragment 复用另有精确几何/结构回归。强制全量 geometry/Scene 开关保留为对照路径。

`preferences_scale` 可用 `--scene` 纳入 Scene 编码，并分别用 `--full-geometry`、`--full-scene` 强制旧路径。2026-10-29 在同机 release、固定 CPU 7、8 panels、每帧一个目标、预热 200 次/测量 1000 次下，将三种模式交错运行三轮，取 p50/p95 的中位数：

| 场景 | 模式 | p50 | p95 | Scene 均值 | flatten visits | fragment build/reuse |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| typing | 全量 geometry + 全量 Scene | 145.20µs | 149.50µs | 94.02µs | 230 | 0 / 0 |
| typing | 局部 geometry + 全量 Scene | 131.74µs | 135.13µs | 93.99µs | 1 | 0 / 0 |
| typing | 局部 geometry + fragment Scene | 44.90µs | 46.77µs | 8.86µs | 1 | 3 / 12 |
| save | 全量 geometry + 全量 Scene | 162.13µs | 168.14µs | 93.24µs | 230 | 0 / 0 |
| save | 局部 geometry + fragment Scene | 58.24µs | 60.93µs | 7.98µs | 0 | 2 / 13 |
| notifications | 全量 geometry + 全量 Scene | 101.75µs | 104.41µs | 94.15µs | 0 | 0 / 0 |
| notifications | 局部 geometry + fragment Scene | 13.87µs | 14.77µs | 7.72µs | 0 | 3 / 12 |

这些数字只覆盖语义事件到 CPU 声明、同步、布局/flatten 和 Scene 编码，不包含 GPU、present、窗口或 input-to-photon 延迟。typing 仍有一次 root Taffy layout；结构、主题等场景会合理回退，不能从局部帧推导所有交互都获得同倍率收益。

`settings_compare --scene` 另测结构更新。2026-10-29 同机 release、固定 CPU 7、预热 200 次/测量 1000 次的结果如下；全量基线与最终实现分别测量，不是三轮中位数：

| 场景 | 模式 | p50 | p95 | layout 均值 | Scene 均值 | flatten visits / patches | fragment build/reuse |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| route | 全量 geometry + 全量 Scene | 358.21µs | 610.47µs | 153.26µs | 37.17µs | 121.3 / 0 | 8.3 / 0 |
| route | 自适应结构路径 | 359.54µs | 613.64µs | 152.02µs | 41.27µs | 121.3 / 0 | 8.3 / 0 |
| advanced | 全量 geometry + 全量 Scene | 76.99µs | 83.19µs | 27.15µs | 26.10µs | 121.3 / 0 | 8.3 / 0 |
| advanced | 局部 geometry + fragment Scene | 76.17µs | 82.90µs | 30.01µs | 19.37µs | 28.5 / 1 | 7.5 / 3 |
| reverse | 全量 geometry + 全量 Scene | 145.51µs | 149.29µs | 45.45µs | 40.47µs | 203 / 0 | 8 / 0 |
| reverse | 自适应结构路径 | 147.42µs | 151.56µs | 48.50µs | 41.22µs | 203 / 0 | 5 / 3 |

route 的大面积页面替换按阈值走全量路径；advanced 的 Scene 编码改善，但布局分析抵消了部分收益；reverse 仍有约 2µs 的整帧回退。这里实现的是可验证的局部失效协议，不宣称结构更新已经普遍加速。



## Skia 原生 Fragment 与局部 damage（2026-10-29）

Skia 的 opt-in tracked damage 不再预先把所有 `Fragment` 展开。Tracker 在顶层比较 Region Fragment：不变时复用 bounds，内容或 transform 变化时保守失效旧、新整个 Fragment bounds；局部重绘可以剔除 damage 外的完整子树，命中的 Fragment 原生递归回放。全量 damage 仍一次展开后回放，因为实测全量路径逐个 Fragment `save/concat/restore` 没有收益。Fragment 内 filter、非默认 blend、Shader 或未知 ink bounds 继续保守回退；当前没有递归求 Fragment 内部的最小 damage。嵌套区域更新也可能使外层大 Fragment 的整个绘制范围失效，因此减少 Scene 编码不等于按同样比例减少重绘面积；应结合真实窗口的 damage coverage 和 CPU/GPU 指标验收。

这不是 Vello/Web 的原生 Fragment 缓存：它们仍沿既有展开/普通 painter 路径。Skia 普通窗口默认跳过完全相同的已显示 Scene（`PLICO_SKIA_PRESENT_SKIP=0` 可对照），局部绘制仍默认全量，只有 `PLICO_SKIA_DAMAGE=1` 等 opt-in 入口使用 tracked damage。2026-10-30 将 damage 外层裁剪显式改为设备整数 `clipIRect` 后，严格 rounded-stroke 回归仍有 2 个 AA 差异像素，因此取整或固定外扩 bounds 本身不是 pixel-perfect 修复。

随后 Tracker 对与 damage 部分相交的顶层 Paragraph、GlyphRun、Stroke 迭代扩张到完整命令 bounds，避免 outer clip 切开 AA 光栅化。Immediate `damage_probe` 的严格 240 帧以及 routed/多面板重复输入矩阵均达到 0 个最终差异像素，同时保留部分 Partial 帧；但 typing 覆盖率和 raster 成本明显增加，具体数据见 performance.md。后续补上 Fragment 原子边界扩张：脏区穿过未变化 Fragment 时也纳入其完整 bounds，避免裁断嵌套文字或描边；CPU 单/多矩形和 Intel Vulkan 的嵌套文字像素对照通过。Fragment 内 shader/filter/非默认 blend 递归检测，持续要求全量重绘，移除效果后的首帧也全量恢复。显式 `draw_scene_damaged` 绕过 Tracker，调用者仍负责安全边界；局部 damage 继续 opt-in，不据此宣称任意手工 damage 输入均安全。

新增像素 oracle 覆盖 Fragment 移动、内部颜色变化、删除、内部 clip、重复不变帧和旧/新 bounds；Reactive Settings 的真实 Fragment 也与全量 Skia redraw 比较。既有 paragraph、稀疏 damage 和交换链历史回归继续保留。

`damage_probe` 在同一次运行中保留“先展开再 tracked draw”的旧后端对照，展开时间计入旧路径。2026-10-29 release、固定 CPU 7、1100×820 的 p50：

| 阶段 | 展开后 tracked | 原生局部 Fragment tracked |
| --- | ---: | ---: |
| idle | 21.4µs | 6.8µs |
| navigation | 24.7µs | 8.7µs |
| sidebar | 2305.9µs | 2480.6µs |
| theme | 792.5µs | 794.2µs |
| text | 25.7µs | 9.2µs |

小更新和空帧明显减少后端遍历；sidebar 本轮仍约慢 7.5%，theme 持平。因此这里验证的是正确性与可选收益，不据此默认启用 damage 或宣称所有动画更快。上述性能数字来自允许有界 AA 舍入的旧探针；2026-10-30 的 whole-AA-command 扩张后，Immediate 版本可用 `PLICO_DAMAGE_STRICT=1` 跑完 240 帧且为 0 个差异像素，新旧性能数字不可直接混用。

## 语义投影与 hit-testing 缓存（2026-10-29）

语义缓存分两层。相同 backend epoch 和对外 revision 的 `semantics_shared` 直接复用 `Arc<SemanticSnapshot>`；局部变化后，普通节点可复用自身角色、名称、值、子 ID 和状态记录。backend commit、输入事件和 semantic action 都推进 epoch；dirty/removed 节点删除对应记录，完整同步清空节点缓存。ButtonBase 的名称来自文字后代、ComboBox 的值来自子节点，这两类暂不缓存。bounds、modal disabled、tooltip、portal controls、focus 和 modal root 始终按当前上下文计算。兼容的 `semantics()` 仍返回按值快照，因此局部重建最终需要 O(N) 拷贝；只有调用 `semantics_shared` 且快照未变时才避免这次拷贝。

Hit-testing 在 placed preorder 上缓存每个子树的 conservative visual AABB 和兄弟/孩子索引。查询仍按原来的逆 preorder 优先级先测后代再测父节点，包围盒外整棵跳过；最终 `visual_contains`、disabled 和 modal 判断不变。reindex、滚动和 presentation transform 更新使索引失效，同帧连续事件复用。`experiment_force_full_semantics` / `experiment_force_full_hit_test` 保留旧路径作对照，不是应用需要使用的 API。

`settings_compare` 的 `--semantics` 和 `pointer` 场景在同机 release、固定 CPU 7、预热 200 次/测量 1000 次下得到：

| 场景 | 缓存路径 p50 | 强制旧路径 p50 | 说明 |
| --- | ---: | ---: | --- |
| idle + semantics | 0.122µs | 18.008µs | 未变 Arc 快照命中 |
| typing + semantics | 39.031µs | 37.231µs | 缓存路径回退约 1.8µs |
| route + semantics | 325.999µs | 341.188µs | 整帧含路由与后端 |
| PointerMove | 1.031µs | 1.298µs | event 均值 0.934µs / 1.176µs |

typing 虽然只构建 11 个节点并复用 54 个，仍有小幅回退；不能把减少构建计数直接当成整帧收益。Seeded differential、控件/路由测试和 Settings 语义几何对照继续通过，另有 Arc 快照、局部节点复用和实际子树剔除断言。

当前收尾边界：布局、Scene、语义和 hit-testing 的失效规则相互关联，仍是主要维护风险。现有 oracle 不是任意事件排列或所有后端像素正确性的证明；本轮停止扩展，不开始 Region 独立布局求解。后续修改应先扩大强制全量差分与真实窗口验收，再增加新的缓存粒度。

## 既有 incremental 示例

```sh
cargo run -p plico-desktop --example incremental
```

`reports/incremental/report.json` 的历史实验使用模拟时钟比较 1000 个文本节点中的单个 opacity 动画和 width 动画。该示例刻意使用拥有所有权的快照接口；实际原生路径通过 `render_retained_frame` 直接消费保留节点。Scene 快照使用 COW 共享命令，修改被共享的快照时才复制，reset 不先复制旧内容。历史报告的具体数字应结合其对应版本阅读，不代表上述 Regions fragment 路径。

更多缓存与输出路径实验见 [cache.md](cache.md)。Reactive 属性 API 支持 `.rx(state, callback)` 和 `.rx((a, b), callback)`，见 [Regions](regions.md#text-属性级绑定)。

### 嵌套 Fragment 的 damage 边界收尾（2026-10-30）

修复前，小色块的脏区穿过未变化的两层 Fragment 内的斜体文字/半透明描边时，
CPU raster 在第 2 帧就与全量绘制不同。Tracker 现在把相交 Fragment 当作不可分割的
绘制命令，迭代扩张到它的完整边界；不新增 Fragment 内部失效表或子树 diff。
代价是重叠的大 Fragment 可能扩大脏区甚至回退全量，因此仍保持 opt-in。

回归覆盖 120 帧、CPU 单/多矩形模式，均严格逐字节相等；另在 Intel Graphics (ARL)
Vulkan 上验证了 60 帧、多矩形和 resize，与全量 GPU 绘制严格相等且仍保留部分重绘帧。
当前 Immediate settings 的严格 240 帧探针仍为 0 个差异像素。

相同 Scene 跳过提交和 damage 检测现在都递归识别 Fragment 内 shader/filter/非默认
blend；这些效果不能仅凭命令相等判断为静止。测试覆盖持续效果与移除后首帧恢复。
显式手工 damage、窗口系统呈现行为及其他 GPU 平台仍需要各自验证，不能将离屏像素
对照当成所有平台的交换链验证。
