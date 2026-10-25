# 无系统标题栏窗口的圆角

launcher / custom headerbar 窗口可以使用：

```rust
WindowOptions::new()
    .decorations(false)
    .corner_radius(14)
```

corner_radius 是逻辑像素，默认 0。launcher 已启用 14px，terminal 没有启用。
设置非零值会请求 alpha-capable native surface，并在整帧内容/HUD 编码后、
截图与 GPU 呈现之前应用圆角裁剪。renderer 清屏色改为透明，窗口内部保留原背景。
因此裁剪的是完整 client area，而不只是某个 UI 容器。

半径随 DPI 缩放并钳位到窗口短边的一半，最大化时取消圆角。
WindowOptions 反序列化时缺少该字段仍采用 0，保持旧配置兼容。

这是视觉圆角，需要桌面合成器支持透明窗口。没有新增平台原生 input-region shaping，
透明角的系统命中区域仍可能是矩形；也没有承诺 OS 原生阴影、装饰样式或 blur/mica。
custom headerbar 的拖动和窗口按钮继续使用既有 WindowCommand 接口，圆角不替代它们。

验证：

```sh
cargo test -p plico-shell window_frame::tests --lib
cargo run -p launcher
```

像素测试用实际 renderer 在 1x/2x 下验证角点 alpha=0，中心和边中点 alpha=255。
launcher-capture 示例也使用同一裁剪函数，以便无窗口验证。
