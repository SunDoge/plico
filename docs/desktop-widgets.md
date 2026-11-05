# Desktop widgets

Gallery 的 Desktop patterns 页面提供 tabs、toolbar、toast 和 drag surface 示例。

## Toast presence

`ToastHost` 保存通知；在视图中调用一次 `toast_viewport(cx, &mut host)`。
Viewport 挂到窗口根节点，因此不会受调用处的小尺寸或滚动裁剪限制。

- 入场和退场使用透明度与 16 px 的垂直位移，时间来自注入的 Clock。
- 时长使用主题的 `interaction_ms`；设为 0 可关闭过渡。
- 关闭后立即停止接受输入，但保留卡片直到退场结束。同 ID 的通知在退场期间重新加入，会从当前进度反向进入，保留节点身份。
- `len()` / `is_empty()` 只统计有效通知；`toast_viewport` 在仍有退场卡片时也返回 true。
- 指针停在卡片正文或按钮上时暂停超时；离开后继续剩余时间。排队而尚未显示的通知不消耗寿命。
- 静止的 sticky 通知以及悬停中的静止通知不持续请求帧；退场结束后也停止请求。

容器通过 `UiContext::hovered_within()` 观察整个卡片的命中范围。
Runtime 只查询主动观察的容器，由 Adapter 反馈可见几何，覆盖非交互正文的悬停。

可重现的离屏入场预览：

```sh
cargo run -p gallery -- --scenario apps/gallery/scenarios/toast-motion.json --capture reports/toast-motion.png
```

核心测试覆盖动画中断、同 ID 复用、闲置后关闭、超时和零时长；Runtime 测试使用虚拟时钟验证真实几何悬停、帧唤醒和 portal 裁剪。

## Shared application composition

参考 MyGO 的 Form / Toolbar / Tabs 分工，应用优先复用组合控件，而不是各自拼面板：

- `panel(cx, "Title").description("Supporting text") + |cx| { ... }`：主题派生的内边距、间距、圆角与标题。用于 Gallery Controls / Lists、backend-settings 的 Profile 和 services-demo。
- `settings_section` / `settings_row`：设置分组、说明和控件行；窄视口下标签与控件上下排列。Gallery Controls 与 Workspace 使用同一组控件。
- `toolbar(cx).wrap(true)` 配合 `toolbar_spacer(cx)`：允许操作换行，并将尾部状态推到另一侧。设置页和任务示例复用同一套操作间距。

这些是 Plico 自身的实现，参考 MyGO 的组合方式；并不意味着已实现 MyGO 的完整键盘导航、工具栏溢出菜单或表单标签关联。控件仍需提供自己的可访问名称。Gallery Patterns 的内容区可滚动，通知 viewport 保持在滚动区外。

设置页宽窄布局和侧边栏动画的离屏检查：

```sh
cargo run -p backend-settings --example sidebar_capture
```

输出位于本地 `reports/sidebar/`。设置页的 immediate 与 Regions 实现通过语义树和控件几何对照测试，避免组合控件迁移造成行为差异。

Gallery 的导航使用 MyGO source-list 风格：单色图标、Workspace / Components / Experiments 分组、单行选中态和图标模式。`Toggle navigation` 切换 224 / 64 px；240 ms 过渡通过注入时钟采样宽度，让旁边的内容区按当前宽度重新布局。动画值放在 transient state，不写入场景配置；配置只保存 `sidebar_collapsed`。当前保留普通按钮的 Tab / Enter 行为，尚未实现 MyGO 的单 Tab stop、方向键和首字母导航。

Patterns 页面同时展示可暂停的 WGSL aurora、通知动画和拖拽区域。导航本身不运行持续 shader，避免闲置时额外请求帧。折叠后的可重现预览：

```sh
mkdir -p reports/sidebar
cargo run -p gallery -- --scenario apps/gallery/scenarios/sidebar-collapsed.json --capture reports/sidebar/gallery-collapsed.png
```
