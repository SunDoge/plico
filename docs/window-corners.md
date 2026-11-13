# 无系统标题栏窗口的轮廓与阴影

launcher / custom headerbar 窗口可以使用：

```rust
WindowOptions::new()
    .decorations(false)
    .corner_radius(14)
    .has_shadow(true)
```

`corner_radius` 是逻辑像素，默认 0；`has_shadow` 默认开启。透明、鼠标穿透的
`floating_overlay()` 会显式关闭阴影。不同桌面系统不再强行共用一套 GPU 裁剪：

- macOS 通过 AppKit/Core Animation 的 content layer 使用 continuous corner，系统阴影
  跟随该轮廓；`titlebar_overlay` 保留 traffic lights、缩放边和原生标题栏行为。
- Windows 的无装饰窗口保留 Win32 sizing/system-menu style，由 winit 的
  `WM_NCCALCSIZE` 路径隐藏 caption；Plico 请求 DWM 阴影，并把任意正半径映射为
  Windows 11 的系统圆角。Windows 10 不伪造任意半径。
- Linux 开启 `linux_csd` 后，在透明 surface 内保留 resize/shadow margin，以 wgpu 绘制
  连续轮廓、边框和可关闭的阴影；最大化和全屏时移除 margin。未开启时将轮廓交给
  compositor/window manager。
- 其他后端仍使用整帧 GPU clip，要求桌面合成器支持透明窗口。

应用只负责 header 内容、窗口按钮和 drag command。Windows/Linux 的 custom resize 使用
既有边缘命中与原生 drag-resize；macOS 双击标题栏遵循系统偏好。运行时
`WindowCommand::SetDecorations` 会同步 Windows DWM frame 与 Linux CSD 尺寸。

当前验证边界：Linux 有 wgpu 像素测试；Windows GNU/MSVC 目标执行交叉编译和策略单测，
真实 Windows 11 的 DWM 轮廓、Snap UI 和多 DPI 仍需要物理系统验收；macOS 原生轮廓由
消费应用在真机验证。

验证：

```sh
cargo test -p plico-shell window_frame::tests --lib
cargo check --target x86_64-pc-windows-msvc -p plico-shell --all-targets
cargo run -p launcher
```
