# 完整与增量 scene 编码实验

目标是判断默认 Vello 后端是否值得维护按节点切片的增量编码，而不是预设
“增量必然更快”或“Rust 重声明免费”。实验保留两边相同的属性 diff、文字缓存、
Taffy 缓存、场景无变化时的整帧复用。

## 当前基准与历史对照

按节点切片的增量实现和运行开关已经删除。下面的 A/B 表是删除前的历史实验，
不是当前仍支持的运行模式。当前保留完整编码与缓存复用的持续基准：

```sh
cargo run --locked --release -p plico-desktop --example scene_work
# 只测一档，或增加人为的全部可见压力场景
cargo run --locked --release -p plico-desktop --example scene_work -- --rows 1000 --unculled
```

当前基准同样输出声明、同步、几何与 scene 分阶段 CSV，以及场景构建/复用次数。
历史 A/B 使用 release、三个串行交替 trial；原始 CSV 属于本地 reports 产物，不参与提交。

这是 CPU 实验，不创建窗口、不提交 GPU、不等待 VSync，不测功耗或 FPS。
首次声明、字体整形与缓存预热不计入稳态。UI 使用注入的 VirtualClock，计时使用
宿主 Instant。没有关闭文字、布局缓存，也没有通过“只统计增量成功帧”挑选样本。

## 对照与正确性

- 正常视口为 800×600；另一档将视口人为拉长到所有行可见，是编码压力测试，
  不代表常见显示器或虚拟列表的实际用法。
- 100 / 1000 / 4000 行；每行有容器和文字，实际节点数约是行数的两倍。
- 八类工作：空闲、无变化重声明、单处颜色、全部颜色、单处文字、单处尺寸、
  容器透明度、条件挂载/卸载。透明度按离散状态切换，未混入宿主过渡动画。
- 每类预热 20 帧，采样 120 帧；报告三个 trial 的统计量中位数。
- CSV 分开记录声明、同步、几何阶段与 scene 时间。现有 sync/geometry 计数是整数
  微秒，geometry 包含布局与后续几何处理，不是纯 Taffy 耗时。
- 实验中的完整路径不记录切片索引，否则会人为高估普通完整编码的成本。
- 比较的是修正图层结束指令后的增量实现；不以错误绘制换取更快的结果。
- 历史 A/B 每类前 8 帧在计时外与完整编码逐条比较所有 PaintCommand，包括 clip、opacity
  和 PopLayer，而不只数文字。三轮共比较 2304 帧，计时采样共 34560 帧。
- 另外的 scene_layers 回归覆盖嵌套 clip/opacity 的像素一致性，以及启用 Save
  按钮后的 Gallery Workspace hover。默认 Save 是 disabled，不能拿它测试有效 hover。

## 解读限制

2026-10-23，Linux x86_64 / Rust 1.99.1 / release，修正图层边界并移除关闭模式
的索引记录开销后，三轮结果如下（µs）：

| 场景 | 完整 scene 均值 | 增量 scene 均值 | 完整 CPU 帧 p50 | 增量 CPU 帧 p50 |
| --- | ---: | ---: | ---: | ---: |
| 正常视口，100 行，单点颜色 | 26.16 | 25.65 | 86.80 | 94.97 |
| 正常视口，1000 行，单点颜色 | 107.04 | 173.53 | 742.36 | 809.79 |
| 正常视口，1000 行，全部颜色 | 119.25 | 293.51 | 1270.78 | 1492.72 |
| 正常视口，1000 行，文字 | 107.06 | 148.85 | 1254.84 | 1313.14 |
| 正常视口，4000 行，单点颜色 | 369.48 | 680.72 | 3225.38 | 3613.96 |
| 全部可见，1000 行，单点颜色 | 772.83 | 578.54 | 1413.77 | 1486.23 |
| 全部可见，4000 行，单点颜色 | 3427.40 | 2828.90 | 6395.91 | 6658.95 |

正常视口只有约 30 行可见；完整编码便宜，增量路径的查找表、重组和切片记录反而
更贵。全部可见的稀疏更新能减少 scene 的平均耗时，但本实现没有改善整帧延迟：
1000 行全部可见的单点更新，CPU 帧 p95 为完整 1432.71、增量 1564.18µs。
注意 scene 列是均值、整体列是 p50，不可将两者直接相减推导其他阶段成本。

两种模式的 Idle 都是 120 帧 scene 复用、0 次 show；Redeclare 都是 120 次 show，
仍然 120 帧 scene 复用。1000 行无变化重声明，CPU 帧 p50 约 406–412µs，scene
约 0.04–0.05µs。这个实验支持接受常规 Rust 重声明成本，而不是取消文字/布局缓存。

**采用的方案：完整编码，保留整帧复用，删除按节点切片的增量实现。**
这不是“所有增量绘制都无用”的证明，也不代表独立子树缓存或 GPU 合成缓存没有价值。
据此已删除增量编码、切片索引及开关；完整编码与无变化时的整帧复用成为唯一路径。

被删除的增量实现在成功后会丢弃 ranges，所以下一次变化需完整编码重建索引。单点颜色和
透明度场景通常是 60 帧增量、60 帧完整；文字、尺寸与结构变化仍走完整编码。
增量路径仍遍历节点、建立查找表并复制完整指令流，不是整条链路 O(Δ)。

全量路径的“无变化帧”也可直接复用上一份 scene；无变化重声明只付声明与提交
检查成本，不会因此自动重新编码。关闭增量编码不等于关闭这些缓存。

完整编码可以作为增量编码的输出基准，但这不是说完整路径不可能存在其他 bug。
离屏像素与指令一致也不能代替用户对实际窗口呈现的确认。

### 无变换节点

2026-11-12 对照 MyGO `Painter.add` 的无变换快速路径检查了 Plico 的
scene 编码。Plico 的普通节点原本即使没有任何静态、动效或宿主变换，也会
构造变换原点的正反平移并做四次仿射矩阵相乘。现在这类节点直接复用
父矩阵；单独设置 `transform-origin` 不会改变无变换节点的结果。

release 下的同进程交替 A/B 每轮各调用 500 万次，三次测得旧路径
5.76 / 5.38 / 5.38 ns/node，快速路径 1.82 / 1.77 / 1.77 ns/node。
4000 行压力场景约 8000 节点，对应约 29–32 µs 的纯 CPU 上限收益。
完整 `scene_work` 运行的机器漂移大于这个数量级，因此不用跨进程帧数据
声称更大收益。手工复现：

```sh
cargo test --release -p plico-renderer profile_untransformed_node_resolution -- --ignored --nocapture
```

随后检查真实 runtime 数据流发现，adapter 曾把普通节点的默认 scale/translate
也编码成 `Some(identity)`，使上述快速路径只在低层 renderer 调用者中生效。
现在 `None` 是运行时 identity 的唯一表示，非默认平移和缩放仍保留完整矩阵。
修复前后相邻的 4000 行全可见运行中，六个重编 workload 的 scene p50 从
约 1.46–1.63 ms 降至 1.33–1.58 ms，各项减少 17–131 µs；跨进程漂移仍意味着
这组完整帧数据只用于确认真实路径已命中，定量结论仍以上面的同进程 A/B 为准。

runtime 原先还把组合后的 world transform 存进独立的 NodeId `HashMap`，供命中和语义
读取；scene encoder 同一帧却再次从局部 Paint 组合整条变换链。现在 presentation geometry
归回 `PlacedNode`，局部 motion 仍只刷新 dirty subtree，runtime scene 直接消费同一结果。
这既删除一份平行所有权和逐节点哈希访问，也保证命中与绘制使用完全相同的矩阵。普通
renderer API 仍保留即时解析路径，已解析入口与它逐条比较 PaintCommand。

4000 行全可见的相邻运行中，五个稳定 workload 的 scene p50 额外减少约 11–71 µs；
文字 workload 在 1.38–1.65 ms 间受主机漂移影响，未计入收益结论。更重要的工作量约束由
`presentation_motion` 保持：1000 个静态兄弟中的两节点 motion 仍只访问两个 placed node，
没有用全表重算换取 renderer 数字。

### 没有活动 scrollbar 的节点

MyGO 会用节点状态跳过与当前布局无关的遍历。对应检查发现，Plico 原本在
每个节点的子树编码后再计算一次 `visual_bounds` 并变换到窗口坐标，只用于
判断 overlay scrollbar 是否可见。绝大多数节点的 scrollbar opacity 为零；
现在先检查这个状态，并复用已经计算的节点可见性结果。scrollbar 位于节点
边框内，因此这也能更早地跳过被祖先 clip 完全遮挡的 scrollbar。

release 下同进程交替 A/B 每轮各检查 500 万个无 scrollbar 节点，三次的
旧路径都是 13.25 ns/node，快速路径为 0.34 / 0.30 / 0.30 ns/node。
8003 个节点对应约 104 µs 的纯 CPU 上限收益。一组紧邻的完整运行中，
六个重编 scene workload 的 p50 均下降 22–137 µs；跨进程结果仍受主机
漂移影响，因此只把同进程 A/B 作为微操作收益的定量证据。手工复现：

```sh
cargo test --release -p plico-renderer profile_hidden_scrollbar_visibility -- --ignored --nocapture
```

### 延迟构造 primitive 的 fallback path

MyGO 的 scene op 保留原始几何，GPU 可以直接识别矩形和圆角矩形。Plico 原本虽然
同样保留了 `Rect` / `RoundedRect` 元数据，`PaintShape::record` 却还会立即分配
一份兼容 Bézier path。主 wgpu 后端只读 primitive，这份 path 在普通 widget 上
从未被使用，但每帧仍要支付生成、分配、命令缓冲 clone 和 drop 成本。

`PaintShape` 现在直接保存 `Rect | RoundedRect | Path` 枚举。wgpu 从前两种变体
直接生成分析 quad；Skia 或任意 path consumer 只在调用 `path_elements` 时才物化
兼容轮廓。任意 path 仍按原 tolerance 在录制时展平，没有改变其精度。

release 下同进程交替 A/B 每轮各录制 100 万个普通背景，旧 eager fallback
路径为 229.59 ns/fill，延迟 primitive 路径为 27.15 ns/fill。在 4000 行、
8003 条命令的完整场景中，六个重编 workload 的 scene p50 从
3.68–3.87 ms 降至 2.38–2.59 ms，各自减少约 1.28–1.47 ms。手工复现：

```sh
cargo test --release -p plico-renderer profile_recorded_background_fill -- --ignored --nocapture
cargo run --locked --release -p plico-desktop --example scene_work -- --rows 4000 --unculled
```

### 复用已编码的文字 scene

MyGO 的文字和 rich-text 路径会保留上一帧可重用的 run，不会因为布局没有
变化仍重建每个 glyph 列表。Plico 虽然已有 `TextContext::glyph_scene` 缓存，
普通 UI 文字却绕过它：每帧重新遍历 Parley line/run，复制 font、style 和
variation，再收集一份 glyph `Vec`。原因是初始实现留下的 Metal fragment
注释，但当前路径会在 backend 之前展开缓存 scene，并不向 wgpu 传递 fragment。

现在普通文字按 layout 实例、device scale 和 synthetic-embolden policy 缓存已编码
scene；普通 UI 仍保留原有的“不几何加粗”策略。`GlyphRun` 的 glyph 和 variation
负载改为不可变 `Arc<[T]>`，所以缓存命令放入当前 scene 只复制小型命令和
增加引用计数，不再深拷贝 glyph 数组。字体注册或后端替换仍会通过现有
`clear_caches` 使其失效。

release 下同进程交替 A/B 每轮各 replay 50 万次 `Retained row A`，
三次直接重编码为 361.52 / 358.82 / 354.69 ns/op，缓存 replay 为
96.75 / 98.74 / 98.39 ns/op。完整 4000 行场景的六个重编 workload，
scene p50 从 2.38–2.59 ms 进一步降至 1.47–1.68 ms，各自减少约
0.87–0.97 ms。这是大量重复文字的压力场景；真实收益取决于可见 layout 的
复用率和 2048 项 glyph-scene 缓存的命中率。手工复现：

```sh
cargo test --release -p plico-renderer profile_cached_glyph_scene_replay -- --ignored --nocapture
cargo run --locked --release -p plico-desktop --example scene_work -- --rows 4000 --unculled
```

### 单遍 preorder 子树编码

MyGO 的 painter 沿节点链做一次深度优先遍历。Plico 的节点虽然是 preorder
扁平数组，旧 scene encoder 却会在进入每个节点前向后搜索其子树结尾，再对子树
重复同样的搜索。普通两三层界面接近线性，但深层组合会反复扫描相同尾部，最坏
退化为 O(n²)。

现在 encoder 用一个共享游标消费 preorder 数组：进入节点后直接递归消费所有更深
的后继，游标停在下一个兄弟；透明或空裁剪子树也只线性跳过一次。子树结束时只需
按计数弹出 clip/opacity layer，不再为每个带 layer 的节点分配临时 `Vec`。

release 下用 256 层链、每轮 2000 棵树做同进程交替 A/B，旧边界搜索为
10126.77 ns/tree，单遍游标为 1076.76 ns/tree，约快 9.4 倍。4000 行压力场景
主要只有两三层，scene p50 仍在原来的约 1.45–1.65 ms 波动区间，因此不把这项
架构修复表述成普通平面列表的帧时间收益。手工复现：

```sh
cargo test --release -p plico-renderer profile_preorder_subtree_walk -- --ignored --nocapture
```

### 子树 ink 裁剪

MyGO 在 commit 后用 `setInk` 把节点自身的 visual overflow 与所有后代的 ink 合并，
`Painter.paintable` 因而可以一次跳过整棵被裁剪区域排除的子树；无法安全界定绘制范围的
节点将 `noCull` 传播给祖先。Plico 现在对宿主已经解析 world transform 的 presented scene
采用相同的保守模型：阴影、outline 和 blur 纳入边界，变换后的溢出后代参与祖先 union；
尚未保留精确 stroked bounds 的 typed vector path，以及没有局部 clip 的 widget scene
会关闭自身及祖先的子树裁剪；这个 `noCull` 规则也适用于不启用预遍历的小场景。

这不是无条件增加一次全树扫描。少于 64 个节点、没有嵌套，或 32 个均匀样本中不足四分
之一落在视口外时，直接走原 encoder。两条路径使用 const-generic 专门化，因此关闭时没有
逐节点的 `Option<ink>` 分支；抽样本身在 8001 节点全部可见场景中为 0.27 µs/frame。
启用时，preorder stack 一次生成每棵子树的结尾和 union bounds，`Source` 持有可复用 scratch，
滚动帧不会反复分配这两个数组。

release 下的同进程交替实验包含 4000 个两节点行（8001 节点，约 25 行可见），并将 ink
预遍历本身计入优化路径：普通逐节点裁剪为 176.75 µs/frame，子树 ink 为
116.92 µs/frame，减少约 34%。完整 `scene_work` 的三轮中位数也确认真实 runtime 路径命中：
单点颜色 scene p50 从 197.22 降到 140.88 µs，单点文字从 197.21 降到
163.73 µs，容器透明度从 192.34 降到 135.46 µs。全部颜色与结构更新收益很小，整帧仍由
声明、同步或布局主导；不据此宣称所有 workload 等比例加速。手工复现：

```sh
cargo test --release -p plico-renderer profile_subtree_ink_culling -- --ignored --nocapture
cargo run --locked --release -p plico-desktop --example scene_work -- --rows 4000
```

### 可见性裁剪

完整编码仍会跳过不可见绘制：零透明度的子树不编码；其他节点按变换后的绘制
包围盒与窗口、祖先裁剪包围盒的交集判断。自身背景先于自身 overflow clip 绘制，
因此自身 clip 只收窄后代的可见区域。阴影和 outline 计入绘制包围盒。
圆角、旋转裁剪使用保守包围盒；旋转后的单轴无限裁剪不收窄可见区域。
父节点离屏不代表后代离屏，允许溢出的后代仍会检查。

这只减少 scene 编码及其调用的文字整形/字形编码，不跳过声明、布局、输入处理，
也不取消布局阶段必要的文字测量。裁剪测试覆盖透明子树与后续兄弟、单轴裁剪、
变换后代、溢出后代，以及被裁文字不新增 scene 文字缓存。
