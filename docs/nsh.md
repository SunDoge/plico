# NSH 最小后端实验

`crates/plico-nsh` 把普通 Plico immediate 声明投影到 [NSH](https://github.com/longbridge/nsh)
0.3 的 element tree。应用仍实现 `plico::DeclareView`，使用普通 Rust 状态、
`#[component]` 和 `+ |cx|`；不需要迁移到 NSH 的响应式组件 API。

职责边界与 LVGL 实验相同：Plico 保留 `Engine` 和声明身份，NSH 负责 keyed
reconciliation、自带 Taffy 布局、原生文字、绘制和平台窗口。该后端不经过 Plico
Scene，也不使用 Plico 的 Taffy、Parley、Vello/Skia 或桌面窗口层。

## 运行

Linux 需要 GTK 4.14+ 及其开发文件。`native` feature 关闭 NSH 平台 crate 的默认
JS feature，因此 Rust-only 示例不链接 QuickJS。

```sh
cargo run --locked -p plico-nsh --features native --example counter
cargo test --locked -p plico-nsh
cargo test --locked -p plico-nsh --features native
```

macOS 和 Windows 的宿主入口使用对应 `nsh-platform-*` crate；当前只在本机 Linux
编译和验证。示例中的 Increment、条件按钮、列表反转和 disabled 都是普通 Plico
声明，不是另一套 gallery。

## 当前实现

- 映射 Root、Column、Row、Text、Button/ButtonBase；Overlay、Dialog、
  TooltipAnchor 目前只保留普通容器结构，不表示已经实现 portal、模态焦点或 tooltip。
- 支持基本像素/百分比尺寸、min/max、grow/shrink、padding、gap、align/justify、
  颜色、背景透明度、边框、圆角、字体、单行 ellipsis、opacity、wrap、clip、
  absolute 位置和 NSH transition。
- 每个节点以世代 `Entity::raw()` 派生 NSH key。动态列表重排仍由 Plico 业务 key
  保留核心身份，再由 NSH 按投影 key 保留原生节点。
- NSH `on_press` 只排队 `Entity` 并触发一个 revision。NSH dynamic region 下一次
  执行时先 dispatch，再运行 `declare_until_settled`，最后重建 element 描述。
- dispatch 拒绝已卸载、世代失效、非按钮和 disabled 目标；事件回调不捕获应用、
  `UiContext` 或声明闭包。
- 不支持的 kind 返回 `UnsupportedKind`，不会退化成无行为容器。高级布局、富文本
  span 和未实现语义返回显式 projection 错误；原生宿主把 declaration/projection
  错误显示在窗口中。

无窗口测试覆盖点击 round-trip、重复 declaration、核心节点身份、keyed reorder、
条件卸载、stale event、disabled input、动态按钮文字和 unsupported kind。native
测试还通过 NSH Core 的 mount batch 验证动态按钮文字确实进入原生 reconciliation，
不只检查 Plico 节点。

Linux 上另在独立 Xvfb 显示中用真实鼠标事件验证：Increment 连击、条件按钮挂载和
点击、keyed 列表反转、Disable/Enable 动态标签，以及禁用时不增加计数、重新启用后
恢复增加计数。Xvfb 验证避免与交互式桌面共享鼠标和窗口；这些检查仍不代替各平台
原生节点身份、窗口生命周期和移动设备测试。

## 未实现

TextInput、SVG/Image、Switch、Checkbox、Slider、Shader、Grid、滚动、富文本 span、
transform、文本选择、portal/modal 语义和高级 `LayoutStyle` 尚未接入。viewport 目前
只在 Root 创建时注入 Plico 声明上下文，未跟随 NSH resize；宿主动画、外部 wake、
IME 和热重载也未接通。当前每次 NSH revision 都重新生成 element 描述，增量原生
修改由 NSH reconciliation 完成；尚未测量双份声明树的内存和更新成本。
