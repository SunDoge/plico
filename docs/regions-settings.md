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

这次证明了“业务代码只执行局部”，没有证明“小页面整次更新更快”。不据此切换默认模型，也没有测出大 N 下的交叉点。

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
