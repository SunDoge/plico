# 完整设置页：immediate 与显式区域对照

本文按实现阶段记录测量。前部表格保留旧基线；当前属性绑定接入与复测见末节。

## 范围与复现

在 backend-settings 内保留原 `ImmediateSettings::preferences`，并由 `preferences_regions` 模块提供独立对照。两版复用同一页面外壳，显示相同控件、文字与样式，包括原生名称输入/预览、通知、advanced 条件分支、保存/重置、主题与 Reduce motion。本文记录的是隔离表单及历史性能实验；完整多路由 `Settings` 后来已迁移为 Reactive，旧实现保留为 `ImmediateSettings`。

```sh
cargo run -p backend-settings --example preferences_regions --features skia
cargo run -p backend-settings --example preferences_regions --features skia -- --immediate
cargo test -p backend-settings --test preferences_regions
cargo run --release -p backend-settings --example preferences_compare -- --iterations 5000
```

基准对每种操作分别创建两版 Source、禁用表现动画、预热 200 次后测量 5000 次。计时包括真实语义事件/文本事件、状态处理、声明/协调以及同一 Adapter 的文本和 Taffy 准备路径；不构建 Scene、不运行窗口、不提交 GPU。这是 CPU 准备耗时，不是完整 input-to-photon 延迟，也不代表 Skia/Vello 绘制性能。无窗口 TextContext 使用默认文字路径，不能把数值当成 SkParagraph 的窗口布局成绩。

typing 交替插入 x / Backspace，避免字符串无限增长；reset 在每次真实编辑之后测量重置，准备编辑的耗时不计入 reset。save 保留真实递增计数与文本变化。advanced/theme 交替开关，双方工作负载相同。结果用 black_box 消费，正确性由独立真实操作测试检查，不仅仅比较 signal setter。

## 本机结果

Intel Core Ultra 9 285K / Linux / rustc 1.99.1 / release；以下为一次本机运行的 p50，单位 µs。宿主负载与 CPU 调频会影响绝对值，不能外推到其他机器或大列表。

| 操作 | Immediate | Regions |
| --- | ---: | ---: |
| 通知开关 | 8.64 | 7.90 |
| 输入编辑 | 18.05 | 23.04 |
| 保存 | 35.31 | 40.93 |
| 编辑后重置 | 19.68 | 25.54 |
| advanced 分支切换 | 39.64 | 45.45 |
| 全局主题切换 | 26.57 | 38.63 |

两条路径使用相同的引擎/后端实现，但实际树形状不同：折叠状态下 immediate 有 24 个 Engine 节点，regions 有 36 个。额外节点来自当前真实 column 区域边界及为保持间距增加的布局分组；这就是此 API 目前实际付出的代价，没有人为移除它们来改善成绩。

例如输入编辑的平均声明阶段约从 3.04µs 降到 1.61µs，但 Adapter 记录的布局准备阶段约从 12.33µs 增到 19.23µs，两版均运行一次布局，flatten 分别访问 24/36 个节点。该阶段包含布局准备等工作，不等于纯 Taffy 求解时间。通知开关两版均不触发 Taffy 重排，区域版才表现出小幅整体收益。保存、reset 和 theme 的区域调度/settle 次数也更多。

CSV 包含 p50/p95、事件时间、声明/同步/布局阶段、flatten 次数、布局次数和声明 pass。现有 Source 的 visited 统计在 full immediate 是最后一个 settle pass，在 regions 是本次 region batch 的合计；CSV 明确标记了这个口径，不能直接把两列除出来当成总执行量的倍率。微秒阶段计时有取整误差，整体 p50/p95 用纳秒计时。不同阶段字段也不是严格可加的分区。

这次证明了“业务代码只执行局部”，没有证明“小页面整次更新更快”。后续已扩展到约 1.5 万节点，见下方规模实验；当前迁移是统一状态模型的架构选择，不表示这些性能结论已经反转。

## 写法与状态分布

原 preferences 函数约 42 行，直接读写 ImmediateSettings 的字段。区域版函数约 76 行，另有根状态工厂、保存状态类型和回调。两者业务控件相同，但区域版多出依赖元组、read/handle、bind/update 与边界。行数受 formatter 和注释影响，仅用来说明当前人体工学成本。

区域版有两份根状态：Theme、Reduce motion；四份页面局部状态：TextBuffer、notifications、advanced、Saved（文字与计数）。状态本体没有在 app/runtime/节点间重复存储；控件使用的 bool 临时副本用于适配既有受控 widget。Saved 与开关使用标记槽支持 checkpoint，TextBuffer 未标记，reload 后重置。原 immediate API 本身也支持 cx.state，所以 local state 并不是区域模型独占的能力；这里对照的是 ImmediateSettings 页面。

工具栏需要 name/saved 的句柄来处理事件，却不需要在 name/saved 变化时重新声明。因此用 `cx.component((name, saved), callback)` 作为非订阅的捕获边界，内部再声明按钮；页面的局部状态仍归外层页面组件所有。这个写法可行，但为了表达“只捕获、不订阅”增加一个有真实布局节点的组件，仍是当前 API 的成本。

新增 `cx.component(deps, callback)` 自动使用调用点身份，静态组件无需手写 key；动态列表继续 component_keyed。新增 `app.theme_state(root_state)` 绑定根 Theme，供普通 UiContext 继承；RegionApp 同时向原生 View 和开发协议转发主题，页面不需要手写一套 View 转发。

## 验收与发现的错误

- 两版执行同一语义脚本：开关、advanced、聚焦/全选/中英文输入、保存两次、重置、主题切换；900px 与 480px 下对照可见控件状态、值、顺序和叶节点几何，容差小于 1 个逻辑像素。
- 通知与输入分别只执行一个 watch，输入 Entity 不变，页面根和事件工具栏不重执行。
- checkpoint 恢复主题、notifications、advanced、Saved 等标记值，输入 buffer 单独重置。
- 静态两次 component 调用获得不同 owner，相同组件函数中的局部状态互不串扰。
- 真实页面暴露了全局主题更新的遗漏：局部 replay 内的事件改变 Theme 后，Source 曾直接提交旧继承样式。现在同一次 try_declare 内检测到主题变化即回退完整声明，不等待第二次外部事件。测试验证切换后第一帧即为新主题。

布局/事件测试不依赖窗口坐标或 GPU；未据此宣称所有平台完成视觉验收。Reduce motion 只验证状态，该独立页没有动效；完整的路由状态保留、宿主异步业务和持续动画仍需单独场景验证。

## 后续判断

这是迁移前的历史判断：这个小页面不支持“默认改为 reactive 会更划算”的结论。当前 Regions 已常驻且完整应用默认采用 Reactive，目标是统一状态模型，而不是宣称它更快。区域边界布局成本、事件捕获与订阅分离、重复 settle 仍是明确的改进对象；高频路径是否使用 immediate 逃生口必须由后续 profiling 决定。

## 规模扩展：完整表单与批量事件

`preferences_scale` 复用上面的真实表单，不改成纯 label/signal setter。分别创建 1、8、32、128、512 个表单，每次修改 1、ceil(10%)、100% 个表单；K 相同时去重，默认共 53 个条件、两版 106 行测量结果。测试 notifications、typing、save、advanced；Theme 是一次影响所有表单的全局广播，单独测量。

```sh
cargo build --release -p backend-settings --example preferences_scale
# 固定同一 CPU 核，避免混合核心调度差异；核号按当前机器调整。
taskset -c 7 target/release/examples/preferences_scale --iterations 100 --warmup 40
# 同一棵区域树关闭 replay，保留真实依赖/组件维护成本。
taskset -c 7 target/release/examples/preferences_scale --updates 1 --full-control
# 普通视口，只操作第一张可见表单。
taskset -c 7 target/release/examples/preferences_scale --updates 1 --viewport-height 780
```

默认视口为 `900 × (780 × 表单数)`，全部表单挂载、可见；不创建这么大的窗口或像素缓冲。它用于测量 retained tree CPU 压力，不是在建议应用不做虚拟化。固定视口模式保持同样的已挂载树，没有引入 list virtualization；scene/光栅化的视口剪枝未计时。

`panels` 是逻辑表单数，`nodes` 是真实 Engine 节点数，`target_panels` / `target_fraction` 是接收事件的表单数/比例，不是受影响像素或实际脏节点比例。advanced 结构变化可能移动后续兄弟表单，主题广播的视觉影响是全局的，不能把这些案例按 O(K) 理解。

通知/保存/分支将 K 个原生语义事件入队后只准备一次 CPU 帧。输入则依次聚焦并修改 K 个原生 buffer，然后统一准备，验证了只有一个输入焦点也能回传多个已修改 buffer。目标 ID 在计时外缓存；事件分发、状态写入、声明/协调、后端准备均在计时内。`mean_backend_update_us` 对应 Adapter 现有 layout_micros 字段，实际包含同步、文本/布局准备和辅助工作，不能视为纯 Taffy 求解，也不能与 sync 时间简单相加。

计时外核对首个测量帧和最终帧的语义指纹，包括所有表单的文本、输入值、checked/disabled 等状态；不同实现的 ID 和额外容器不参与比较。自动测试还对 1/4/16 个表单检查每次恰好 K 个开关变化、其余兄弟不变、批量编辑/保存、分支反复装卸、几何一致、输入身份稳定，以及节点数量不泄漏。基准本身也核对大规模条件的结果，而不是只依赖小规模测试。

### 三种路径的含义

- `immediate`：原 Settings::preferences + 稳定 ID + 现有 retained 后端，作为实际基线。
- `regions`：当前显式区域 API，正常局部重放。
- `regions_full`：同一套区域状态、回调、组件和布局树，故意禁用局部 replay。用于隔离“跳过执行”的收益；仍支付区域登记/订阅维护成本，不能冒充普通 immediate。

折叠状态下，普通基线约 `20P + 5` 个节点，区域版约 `30P + 6`；P=512 时分别为 10,245 / 15,366。局部与 regions_full 的节点数完全相同，行为由测试核对。没有通过降低区域版的控件数量来制造性能优势。

### 测量结果

以下规模数据使用与前文相同机器、release、固定 CPU 7；不能与前文未固定核心的小页面数值混为同一轮测量。阶段数据是现有诊断字段，整体 p50/p95 为纳秒高精度计时。CPU 频率和其他进程仍会带来波动，不给一个脱离场景的精确“交叉节点数”。

一次固定核矩阵运行的 p50，单位 µs；K=1（仅一张表单接收局部事件）：

| 表单 P | 节点 I / R | 通知 I / R | 输入 I / R | 保存 I / R |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 25 / 36 | 9.8 / 8.4 | 19.8 / 24.6 | 36.4 / 41.4 |
| 8 | 165 / 246 | 54.1 / 44.0 | 103.2 / 116.9 | 124.1 / 134.6 |
| 32 | 645 / 966 | 220.1 / 175.3 | 422.7 / 477.7 | 447.5 / 509.7 |
| 128 | 2565 / 3846 | 918.2 / 863.1 | 1812.7 / 2482.0 | 1829.4 / 2487.4 |
| 512 | 10245 / 15366 | 4303.0 / 4353.7 | 14123.6 / 20711.1 | 14291.2 / 20535.3 |

I 为普通 immediate，R 为局部 regions。通知在中等规模有收益，到 512 表单时两版基本持平；输入/保存到约 1.5 万节点仍未出现区域版整帧准备更快的交叉点。

同样一轮中，P=128 的更新密度对照（p50，µs）：

| K | 通知 I / R | 输入 I / R | advanced I / R |
| ---: | ---: | ---: | ---: |
| 1 | 918.2 / 863.1 | 1812.7 / 2482.0 | 1905.0 / 2673.9 |
| 13 | 948.6 / 891.3 | 1860.0 / 2616.5 | 2277.0 / 3204.6 |
| 128 | 1130.7 / 1198.9 | 2391.3 / 3514.8 | 6213.5 / 7599.7 |

P=512、K=1 输入时，声明均值约 1961.66 → 29.89µs；后端准备约 11785.84 → 20111.35µs。两版均做一次布局，flatten 分别访问 10245.0 / 15366.0 个节点。局部重放的声明收益真实存在，但不是整个管线的收益。

同核、同样 100 次测量的独立同构树对照（p50，µs）：

| P / 操作 | 普通 immediate | 局部 regions | 强制全量 regions_full |
| --- | ---: | ---: | ---: |
| 32 / 通知 | 211.6 | 174.6 | 470.4 |
| 32 / 输入 | 417.8 | 484.7 | 788.1 |
| 512 / 通知 | 4275.1 | 4375.2 | 11868.0 |
| 512 / 输入 | 14300.0 | 20677.9 | 27737.4 |

固定 780px 视口、P=512、K=1 的同核复核（p50，µs）：

- 通知：immediate 4425.0，regions 4692.2。
- 输入：immediate 14550.0，regions 22087.0。

这不是“规模还不够，所以肯定更大就会赢”的证据。只改开关时，两版甚至都没有 Taffy 重排，后端更新依然随 N 增长。输入变化则保留一次布局和全量 flatten；跳过页面声明并没有顺带跳过这些阶段。

同构树的 regions_full 对照能确认局部 replay 的执行收益，但与原生基线对比时，额外约 50% 的布局节点、组件调度、后端全树辅助工作仍是实际成本。固定 780px 视口的独立测量也出现相同趋势，不能简单归因于大视口设置。

### 更明确的后续优化对象

1. 更新边界和布局节点分离：避免为 watch/component 额外增加真实 column，而不是仅优化 State setter。
2. 后端更新的全树辅助工作：例如当前滚动状态、selection refresh、控件对齐等路径。源码中存在遍历 placed 的路径，但此实验没有做采样 profiler，不能把阶段耗时精确分摊给这些函数。
3. 文本/结构变化后的全量 flatten 与布局传播：需要测量可缩小的范围，不能承诺天然 O(K)。
4. 全局 Theme 失效尽早回退完整声明：当前先重放受影响区域再统一刷新，P=512 时可产生 514 个声明 pass；这类广播本来就不应当按稀疏更新宣传。

因此，前面的纯执行基准与真实页面结果并不矛盾：前者验证“能省掉声明执行”，这里验证“省下的时间有没有超过整个管线新增的成本”。当前实现不存在统一、单调的 N/K 交叉点；不能据此否定区域模型，也不能据此宣布其已经比默认 immediate 更快。

## Adapter 局部工作优化后的复测

上面的规模表是 `150c0fd` 时的优化前基线。本节为后续共享 Adapter 优化，默认 immediate 和 regions 都受益；没有更改表单、节点数量、分辨率、布局算法或 Skia damage 开关，也没有重新引入增量 scene/flatten。

保留的改动：

- 在同步/卸载节点时维护 slider、shader、tooltip、selectable 所有者索引，无实例时不再扫描所有 placed 节点。
- selection 按几何/结构/相关文字或容器变化刷新；无关叶控件的 paint 不重建其他文档的选择模型。关闭最后一个 selectable 所有者仍会清除旧模型与高亮。
- 滚动容器投影索引随现有 reindex 一起重建，视口反馈和滚动条状态只遍历滚动容器。
- 原生输入使用每节点单份绘制片段缓存。只有本地尺寸、完整 buffer presentation revision、焦点、placeholder、字体、文字颜色、Theme 或 TextContext 缓存代数变化时重建；绝对位置变化不重建本地片段，但 IME 屏幕位置继续从当前布局计算。
- TextContext 提供进程内缓存代数：注册字体、换 paragraph engine、清缓存和更换 context 都能使输入缓存失效；不把这个标识序列化或当跨模块 ABI。

另外尝试了控件文字度量缓存，但同核结果未显示稳定收益，没有保留额外缓存结构。

固定 CPU 7、release、预热 40 次、测量 100 次，512 个表单、只操作一张，p50 单位 ms：

| 操作 | Immediate 前 / 后 | Regions 前 / 后 |
| --- | ---: | ---: |
| 通知开关 | 4.303 / 1.784 | 4.354 / 0.026 |
| 输入编辑 | 14.124 / 9.739 | 20.711 / 12.475 |
| 保存 | 14.291 / 9.663 | 20.535 / 12.311 |
| advanced 分支 | 14.787 / 10.826 | 21.307 / 14.373 |

这些是 CPU 准备路径，不包含 Scene 编码或 Skia/GPU 光栅化，不等于应用总帧时间的同倍数提升。节点数仍为 10,245 / 15,366。通知不重排时，局部声明的收益现在能兑现；输入/结构变化仍执行必要的布局和全量 flatten，区域版目前依然比普通基线慢。

复测命令：

```sh
cargo build --release -p backend-settings --example preferences_scale
taskset -c 7 target/release/examples/preferences_scale --sizes 32,128,512 --updates 1,100% --scenarios notifications,typing,save,advanced --iterations 100 --warmup 40
```

新增 CSV / tracing 工作计数：selection_visits、slider_visits、shader_visits、tooltip_visits、input_rebuilds/input_reuses、scroll_visits；Source 的 BatchStats 另有 scroll_viewport_visits。512 表单输入编辑只重建 1 个输入片段，复用其余 511 个；通知场景上述四类非相关访问为 0，滚动状态只访问 1 个容器。

验证覆盖：输入移动时 fragment 复用与 IME 跟随、单 buffer 编辑、preedit、替换 buffer、placeholder/尺寸/字体/Theme/context 失效、selectable 开关清理、无关控件不重建选择、scroll 开关后索引清理、shader 挂载/卸载。现有输入、文字选择、滚动、虚拟列表、popover/modal、对齐、slider 与 tooltip 非 GPU 回归通过；规模复测也核对首帧/最终帧状态指纹。

回归中发现虚拟列表旧断言把 composable button 的节点数当成 leaf button；在原 `150c0fd` checkout 上复现同样失败后，将断言改为按实际挂载行数限制节点预算，保留精确行窗口和滚动位置检查。既有 shader / 带 shader tooltip 的像素测试，在当前默认 Skia 配置下仍报 Unsupported(Shader)，未将它们记为通过；本次 shader 索引另外用无 GPU 原生路径验证。

## 原生窗口交互验收（2026-10-29）

在 `b07d5a4` 的代码上，使用独立 Xvfb/X11 显示、Skia Vulkan 的 llvmpipe 设备，运行真实 winit 窗口；通过 xdotool 注入操作系统鼠标/键盘事件，ImageMagick 捕获窗口像素，Tesseract 核对编辑预览、保存结果与重置文本。这一轮覆盖窗口输入与 swapchain 呈现，不是 `Driver` 的无窗口模拟。

Preferences 对照依次执行：初始状态 → 全选并输入 `Region window verified` → Save → 展开 advanced → Dark theme → 缩至 480×600 → 向下滚动至 Reduce motion → 恢复 900×780 并 Reset name → unmap/map → advanced 连续展开/收起 12 轮。每个状态先移开指针、清除编辑焦点，再截取同尺寸窗口图，排除 caret 闪烁和 hover 对比较的干扰。

分别运行 immediate、regions（damage 关闭）、regions（damage 开启）三组。10 个对应状态的 immediate/regions 比较，以及 regions 的 damage 开关比较，共 20 对截图，RMSE 均为 0。编辑预览与保存/重置文字也通过 OCR 断言；日志未出现 panic、presentation failure 或 Vulkan validation 错误。未将生成截图与日志纳入仓库，也未增加 CI 窗口任务。

另运行 `plico-runtime` 的 `regions` 原生窗口示例，开启 damage，实际验证：全局计数与嵌套 Double 同步更新；两个组件 local count 分别为 2 和 1；第一个组件宿主异步请求从 Idle/Loading 返回 Loaded，第二个仍为 Idle；输入任务并 Enter 提交从 3 项增至 4 项；Reverse 后新任务排到首位，删除后恢复 3 项；拖动 slider 将 Intensity 改到 90% 并更新文字样式；折叠嵌套区域期间把 Count 从 3 改到 5，再展开显示 Double: 10，已有 local count 保留；缩至 760×600 后外层可滚动。

复现入口：

```sh
cargo run -p backend-settings --example preferences_regions --features skia
cargo run -p backend-settings --example preferences_regions --features skia -- --immediate
PLICO_SKIA_DAMAGE=1 cargo run -p backend-settings --example preferences_regions --features skia
PLICO_SKIA_DAMAGE=1 cargo run -p plico-runtime --example regions,plico-shell/skia
```

限制：这些结果来自 Linux X11 的软件 Vulkan 驱动，不代表 Wayland、独立显卡、Windows/macOS 或真实中文输入法已经验收。Preferences 示例没有持续动画，静态帧比较不证明动画中间帧、帧间延迟或帧率；异步示例仅验证结果唤醒，不验证真实网络的取消/失败行为。GUI 列表验收检查可见顺序和结果，State owner 的释放与旧句柄拒绝仍由生命周期回归测试覆盖。

## 属性绑定接入页面（2026-10-29，基线 c6d61d8）

设置页的名称预览与保存提示已从区域声明改为 Text `.reactive(...)`：名称标签是静态节点，输入更新执行编辑器 watch 和预览属性绑定；保存提示不再有独立布局容器。编辑器仍使用原生 TextBuffer/bind，工具栏仍走现有事件协议，主题仍由根 Theme 注入。完整页面对照、窄屏、输入身份保持、marked-state checkpoint 恢复和多表单批量更新测试继续覆盖这些改动。

折叠页面节点从 36 降到 34。输入更新的 region batch 声明 visited 从 8 降到 2；属性回调单独运行不计入节点声明 visited，但仍有依赖调度与回调执行成本，不能把这个比例当成总 CPU 加速比。

在同机 release、固定 CPU 7 下，对基线和修改版交错运行三轮；每种交互先预热 200 次，再测 1000 次，取三轮 p50 的中位数。沿用 `preferences_compare` 的事件 → CPU 帧准备计时，不含窗口、Scene 或 GPU。重新构建后核对二进制哈希不同，并核对节点计数 36/34；曾因共享 target 误复用同一二进制的初次结果已丢弃。

| Region 页面操作 | 改前 µs | 改后 µs |
| --- | ---: | ---: |
| 通知 | 2.52 | 2.59 |
| 输入 | 17.90 | 17.31 |
| 保存 | 33.99 | 33.87 |
| 重置 | 20.57 | 19.82 |
| advanced | 37.55 | 40.24 |
| 主题 | 33.83 | 33.73 |

这轮只支持“声明范围和节点数减少”的结论，不能宣布端到端普遍提速。同步控制组的输入也从 14.79 变为 15.81µs，advanced 从 34.08 变为 36.31µs，提示微小差异会受编译布局与运行负载等因素影响。布局、flatten 和输入管线仍需处理必要工作；小页面仍不支持强制所有代码改用 reactive 的结论。

```sh
cargo build --release -p backend-settings --example preferences_compare
taskset -c 7 target/release/examples/preferences_compare --iterations 1000
```

跨 checkout 对比应使用独立 target 或明确强制重建，并验证产物哈希和工作计数；不能仅凭 cargo 成功退出认定测量了不同版本。

属性版页面再次完成 X11/Skia llvmpipe 窗口验收：同步/区域以及 damage 开关的 20 对同状态截图仍为 RMSE=0。原生 regions 示例还通过滑块和开关驱动 `.reactive` 的字号、字重、颜色，窗口 OCR 确认 Intensity 为 90%、预览仍显示；计数、宿主 Loaded 唤醒、条件卸载与恢复、resize 也通过。截图与日志只保留在本地 reports，不加入 CI 或仓库。
