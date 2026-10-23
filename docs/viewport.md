# 宿主缩放与 viewport

窗口尺寸由宿主传给 Engine 与布局后端，组件通过普通布局属性适配，不需要 viewport Signal。
ZoomSource 在宿主层统一缩放布局、绘制、命中测试和输入法几何；缩放状态不要求写入业务 checkpoint。

```sh
cargo run -p plico-gallery --example zoom
```

示例使用 ZoomController 读取 factor 和设置缩放；支持 Ctrl + / Ctrl - / Ctrl 0，
macOS 也支持 Command。组件读取控制器值时仍是普通声明，不建立依赖图。
具体实现见 crates/plico-vello/src/zoom.rs 和 apps/gallery/examples/zoom.rs。
