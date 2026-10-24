# 状态样式与动画权重实验

这是 `crates/plico-vello/examples/state_style.rs` 中的独立实验，未增加正式 API，
未复制 Glacex 代码，也未引入依赖。参考的是交互目标值与连续状态权重分离的设计。

运行：

```sh
cargo test --locked -p plico-vello --example state_style
cargo run --locked -p plico-vello --example state_style
```

程序在 stdout 输出 181 个虚拟时钟采样点，在本地 reports 目录生成状态对照图。
截图需要可用的 GPU 后端；测试无需窗口或 GPU。输入是脚本模拟，不是鼠标交互。
示例包含按钮、标签页外观与滚动条滑块外观；后两者只是样式投影，未实现真实切页或拖动。

## 展开目标

- `StateStyle<N>` 保存 idle 与 hover / selected / focus / press / disabled 的可选目标。
- 宿主拥有 `Animator`，每次输入变化后，用注入时钟的 delta 推进五个权重。
- resolver 是纯函数，按上述顺序叠加目标，不读状态、不启动任务、不订阅业务数据。
- disabled 清除其他交互目标，但已有权重会平滑退出。
- 每个状态权重采用指数趋近；选中半衰期 60ms，其余为 45ms。差距不超过 0.001
  时吸附终值，返回 inactive，以便未来接入宿主按需唤醒。
- 采样结果直接映射到现有 builder 属性；关闭控件原有 motion，避免双重插值。

同一份权重可以驱动不同外观，因而无需为颜色、缩放和边框分别创建动画。
采样连续权重意味着中途改变目标无需重新创建 tween。

## 已验证

五项测试覆盖状态优先级和缺省回退、不同步长的指数结果一致性、打断连续性、
最终收敛、禁用目标，以及真实 Source/Engine 下的布局复用。

最后一项将三种外观连续更新 30 帧，每帧断言 `created == 0`、`layout_runs == 0`。
它验证纯绘制属性不触发重排，不代表没有声明、树检查或 scene 编码成本。

默认脚本在第 60 帧取消所有交互，第 86 帧返回 inactive，约 433ms 后停止要求动画。
这里只验证采样器的 active 返回值，未修改正式宿主调度。

## 结论与边界

这个抽象可以放在现有 immediate 声明和保留缓存之间，不需要 reactive graph。
值得继续验证，但尚不足以直接推广到所有属性：

- 示例使用 RGB 简单插值；透明色、预乘 alpha、感知色彩空间和主题 token 动态解析未验证。
- 示例仅包含颜色、缩放、边框宽度；不含影响布局的字号和 padding。
- 边框作用于固定尺寸控件，不能据此推广所有控件的边框变化都无需重排。
- 真实集成需要以 Entity/调用点身份保存权重，处理卸载、reduced motion、主题切换、
  热重载重置和动画唤醒。暂不把这些状态加入可序列化业务状态。
- 本次没有做性能对照，不宣称比现有 motion 更快；主要验证 API 分层与行为。

下一步宜挑一个正式控件接入内部状态样式解析，对比原有 hover/press 动画，
确认没有额外声明次数和双重动画后，再决定公开 API。

## 已接入按钮的内部机制

Vello 后端已在 `button_state.rs` 中引入宿主拥有、Entity 索引的 hover / press /
focus / disabled 权重。旧文本按钮与 `button(cx).text(...)`、自定义 children 的
样式按钮均接入；无样式 `button_base` 继续保留原有行为，不擅自增加背景或禁用淡化。
核心用 `Style::styled_button` 区分样式组合按钮，并保留其他后端原来的禁用投影。

正式实现保留 `Motion` / `transition(ms)` 的完成时限，使用有限时长 cubic ease-out，
没有照搬实验中的半衰期时长。打断时从已呈现权重继续，完成时精确吸附终值。
现有背景 token、显式 hover/pressed background 继续有效；默认背景颜色目标在
进入 RGB 插值前解析，避免丢失 Control / Accent 的默认状态色。

颜色、焦点环透明度和禁用透明度使用同一组权重；背景不再同时接收旧 hover/press
颜色 tween。焦点环宽度保持 2px，渐变其 alpha，不改变布局。禁用命中和语义立即
生效，视觉淡化随后完成。活动集合只采样正在过渡的按钮；最后一帧仍传播脏节点。

reduced motion 或零时长直接应用目标；卸载删除条目，主题替换清除权重并按当前
交互状态重建，避免对主题 reveal 再做交互 crossfade。状态只留在宿主，不进入
业务 Serde 状态；重建整个宿主时自然重置，不新增热重载协议。

范围仍限于 Vello 按钮内部；未公开 `Stateful<T>`、未扩展到布局属性，LVGL 保留
自身投影。本次验证不构成跨后端视觉一致性或实际窗口验收。
