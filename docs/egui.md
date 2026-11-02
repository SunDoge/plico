# egui 原生控件后端（实验）

本后端使用 egui 0.37 的高层控件和文字系统，Plico 提供普通函数声明、稳定节点身份、
应用状态与 Taffy 外部布局。它不回放 Plico Scene，也不把 Parley glyph 转成纹理。
旧 Scene → epaint 实验和自制字形/阴影光栅化已移除；原 `replay` feature 改为 `widgets`，
`window` 会自动启用它。

```text
Plico View::show → Engine → Taffy 分配矩形 → egui 控件 → egui-wgpu
                                  ↑              │
                                  └─ egui 测量文字
```

egui 的同一个 Context 测量和绘制文字，使用内置字体、系统字体 provider、原生字体图集
和缓存；SVG 使用 egui_extras 的图片加载器。Plico 不维护另一套字形缓存。
Taffy 样式转换由可选的 `plico/taffy` 模块提供，FLTK 也复用该模块。
默认 core 构建不引入 Taffy。共享 shell/renderer 仍依赖原有的绘制类型和文字库，
本实验没有声称消除了它们的编译依赖。

## 运行

```sh
cargo run -p backend-settings --features egui -- --backend egui
cargo test -p plico-egui --features widgets
```

接到共享窗口时，用 `plico_egui::Source::new(view)` 包装高级 `plico_runtime::HostView`，
窗口选择 `RenderBackend::Egui`。该 Source 通过 native-widget 帧入口声明，
不会进入 Scene 编码或 Parley 文字测量。其他后端的 Source 不受影响。

控件的 egui ID 派生自 Engine Entity。egui Response 将点击、滑块数值与编辑内容
送回 Engine，下一次声明消费事件；回调不捕获应用状态。TextBuffer 的外部修改仍然有效。
原生输入由 egui 处理，支持焦点、键盘、剪贴板以及基础 IME preedit/commit 和候选框位置。
重绘期限由 egui 返回，宿主按需唤醒，不为光标闪烁维持持续 60 Hz 循环。

## 当前边界

- Text、Button/组合按钮、Checkbox、TextInput、Slider、SVG 和容器已映射。
  Switch 暂投影成 egui Checkbox。滚动由 egui ScrollArea 管理。
- Taffy 管控件的外部矩形，egui 控制内部排版、字体与交互外观；不保证与 Skia 像素一致。
- Shader、Image 和密码输入明确拒绝，不伪装成完整支持。编辑器、terminal、video 等
  renderer Widget 也尚未接入。不要将本实验作为这些能力的替代后端。
- 自动样式过渡、复杂 transform、圆角子树裁剪、tooltip/modal 的完整语义，以及
  accessibility 和高级 IME surrounding-text/selection 协议仍需完善。
- 控件离屏测试覆盖点击、禁用、文本编辑、外部值变更、插入节点后的编辑器身份和
  viewport 重排；不把 mesh 生成成功当作原生控件验收。
- 原生窗口在 Linux Xvfb 中人工验证；没有增加桌面自动化 CI 或 Python 验收脚本。
