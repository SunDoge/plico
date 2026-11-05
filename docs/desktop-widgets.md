# Desktop widgets

Gallery 的 Desktop patterns 页面提供 tabs、toolbar、toast、drag surface、breadcrumbs、
meter、spinner、rating、stepper 和 split 示例。

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


## Split 与 Stepper

`split` / `split_vertical` / `split_percent` / `split_vertical_percent` 参考 MyGO
`ui/split.go`。应用持有第一栏的像素尺寸或百分比，divider 拖动时更新该值；像素尺寸
只保存用户选择，窗口变窄时 `shown` 会按主题 `space(10)` 的最小栏宽夹取，窗口恢复后
仍用原值。百分比模式把拖动换算成 0..100 的份额，两栏随窗口按比例缩放。

- divider 是 6 px grip 中的 1 px 线，拖动时使用 transient state；checkpoint 不恢复，
  释放后下一帧从按下位置重新锚定。
- 共享空间取窗口逻辑宽度/高度，而不是 split 自身的布局盒；嵌套 split 或带 padding 的
  容器会按窗口而不是容器夹取。
- 尚未接入 divider 聚焦后的方向键移动；grip 布局在两栏之间，MyGO 将其覆盖在栏上以便
  不改变两栏的可用宽度。拖拽结束后不再请求帧，持续拖动由指针事件驱动。

`stepper` 参考 MyGO `ui/indicators.go`：一对箭头按钮把 `f64` 吸附到以 low 为锚的
step 网格并夹在范围内，按下立即步进，按住 400 ms 后每 80 ms 重复（MyGO 的加速曲线
相同）。按下与点击分开处理，所以松手不会重复步进；键盘/辅助技术激活按一次点击处理。

- 两个箭头是独立焦点，名称分别为 Increase 和 Decrease；容器带 spinbutton 角色。
  MyGO 用一个 `.Label` 命名整个 stepper，并支持 Up/Down/Home/End，尚未移植。
- 吸附同时清除 0.1 这类步长的浮点误差，显示值仍是普通 `f64`。
