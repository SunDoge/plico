# 完整设置页：immediate 与显式区域对照

## 范围与复现

在 backend-settings 内保留原 `Settings::preferences`，并新增 `preferences_regions` 模块。两版复用同一独立页面外壳，显示相同控件、文字与样式，包括原生名称输入/预览、通知、advanced 条件分支、保存/重置、主题与 Reduce motion。不是整个多路由应用的迁移；原应用默认行为不变。

```sh
cargo run -p backend-settings --example preferences_regions --features regions,skia
cargo run -p backend-settings --example preferences_regions --features regions,skia -- --immediate
cargo test -p backend-settings --features regions --test preferences_regions
cargo run --release -p backend-settings --example preferences_compare --features regions -- --iterations 5000
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

这次证明了“业务代码只执行局部”，没有证明“小页面整次更新更快”。不据此切换默认模型。后续已扩展到约 1.5 万节点，见下方规模实验。

## 写法与状态分布

原 preferences 函数约 42 行，直接读写 Settings 的字段。区域版函数约 76 行，另有根状态工厂、保存状态类型和回调。两者业务控件相同，但区域版多出依赖元组、read/handle、bind/update 与边界。行数受 formatter 和注释影响，仅用来说明当前人体工学成本。

区域版有两份根状态：Theme、Reduce motion；四份页面局部状态：TextBuffer、notifications、advanced、Saved（文字与计数）。状态本体没有在 app/runtime/节点间重复存储；控件使用的 bool 临时副本用于适配既有受控 widget。Saved 与开关使用标记槽支持 checkpoint，TextBuffer 未标记，reload 后重置。原 immediate API 本身也支持 cx.state，所以 local state 并不是区域模型独占的能力；这里对照的是现有 Settings 页面。

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

目前应继续把 regions 留作可选实验。明确的改进对象是区域边界的布局成本、事件捕获与订阅的分离，以及重复 settle；不应为漂亮的 API 先引入自动依赖图或额外宏。是否值得继续扩大迁移，应在这些成本被测量和改善后再判断。这个小页面不支持“默认改为 reactive 会更划算”的结论。

## 规模扩展：完整表单与批量事件

`preferences_scale` 复用上面的真实表单，不改成纯 label/signal setter。分别创建 1、8、32、128、512 个表单，每次修改 1、ceil(10%)、100% 个表单；K 相同时去重，默认共 53 个条件、两版 106 行测量结果。测试 notifications、typing、save、advanced；Theme 是一次影响所有表单的全局广播，单独测量。

```sh
cargo build --release -p backend-settings --example preferences_scale --features regions
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
