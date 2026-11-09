# macOS 与 MyGO 的优化对照

优先参考本地 MyGO `ee3d4bd9bd741f197de231b9fd9b002e9db55577` 的实现，
将通用行为适配到 Plico 的 Rust runtime / desktop host；不是逐字复写 Go 后端。
原生窗口移植的许可证和来源保存在 `crates/plico-shell/third_party/`。

## 已实现

| MyGO 参考 | Plico 行为 | 验证范围 |
| --- | --- | --- |
| `internal/darwin/window.go::roundContent` | AppKit content layer 连续圆角、清空 NSWindow backing、系统阴影、最大化时半径归零 | 原生模块通过 Apple Silicon 目标类型检查与 Clippy；macOS 实机画质由用户验收 |
| `internal/darwin/surface.go::RequestFrame` / `visible` | 隐藏或被遮挡窗口不维持自动逐帧请求；恢复可见后请求完整帧；异步宿主服务继续处理 | 共享调度器覆盖隐藏、到期、未来 deadline、恢复动画；单窗口与多窗口共用策略 |
| 同文件 display link 空闲暂停的调度原则 | `request_rebuild_at` 映射到桌面事件循环 `WaitUntil`；纯计时器不作为持续动画 | 注入时钟验证最早 deadline、稳定绝对时间、组件移除取消、与真实动画并存 |
| `ui/input.go::scroll` 的 `glide := !precise` | 精确触控板位移立即生效，鼠标刻度按配置平滑；精确事件取消已有滚轮动画，基于已呈现位置继续 | 像素小数增量、正反方向、零增量接管、取消旧目标、虚拟列表与嵌套滚动回归 |
| 空闲暂停原则在 runtime 的适配 | tooltip 的 600ms 延迟、滚动条的 800ms 停留使用 deadline；只有实际淡出期间采样帧 | 599/600ms tooltip，799/800/900/1000ms scrollbar，静止无重声明或布局，结束/移除清理 |
| `internal/darwin/{system,app}.go` 和 `ui/preferences.go` | NSWorkspace 初始快照和变化通知，UiContext 可读；系统减少动效不被应用 false 覆盖、玻璃自动不透明回退、语义边框和次要文字增强对比 | 原生模块 Apple Silicon 类型/Clippy 检查；注入偏好测试覆盖恢复、无累积调色和空闲复用；系统设置实机通知仍待用户验收 |

精确滚动的 `Pixel` 输入已包含平台提供的细粒度移动和惯性，不再叠加 160ms 插值。
`Line` 输入的 delta 仍由宿主归一化为逻辑像素，`smooth_scroll` 控制其插值。
测试中的平滑滚轮输入明确标记为 `Line`，避免把错误的触控板延迟当作预期行为。

浏览器暂时仍以 rAF 轮询延迟任务，维持既有行为；这轮没有迁移 CADisplayLink，
没有声称完全复刻 MyGO 的呈现时序，也没有 macOS 耗电或 FPS 实测数字。

## 后续检查

- 对照 MyGO 的 FirstFrame / OnReadyToShow，检查窗口首次呈现前曝光和空白闪烁。
- 继续核对自定义应用颜色是否采用语义 token；自绘控件可读取 `UiContext::display_preferences()`。
- 对照 macOS live resize 的事务呈现，检查连续缩放和跨屏 DPI 变化时的重复配置与资源生命周期。

这些是仍待检查的项目，不是已经实现的能力。

## 原生辅助功能偏好

桌面 `WindowMetrics` 传递 `reduced_motion`、`reduced_transparency` 和 `high_contrast`。
macOS 通过通知唤醒后才重新读取设置；没有周期性计时器。runtime 将系统请求和应用的
减少动效选项按 OR 合并，系统关闭请求时保留应用自己的选项。

`UiContext::display_preferences()` 给自定义组件读取原生请求。`LiquidGlass` 自动使用
不透明 fallback；Modal 在系统减少动效时直接完成挂载和卸载。高对比度按 MyGO 的比例
增强 `Theme.border` 和 `Theme.muted`，保留应用品牌色；每次从原始主题计算，避免反复
声明导致颜色持续漂移。应用硬编码的颜色不会自动变换，推荐使用语义颜色 token。
