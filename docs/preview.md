# 场景预览与开发循环

当前开发路径是静态编译、自动重启与确定性离屏截图。桌面动态库加载、stabby ABI、模块状态迁移和对应 feature 已移除。重启会重建应用；需要保留的数据由应用正常持久化。

```sh
cargo plico dev apps/gallery
cargo plico capture apps/gallery --scenario apps/gallery/scenarios/controls.json --output reports/controls.png
cargo plico capture apps/gallery --scenario apps/gallery/scenarios/controls.json --output reports/controls.png --watch
```

`dev` 监听源码，编译成功后关闭旧进程并启动新程序。编译失败保留旧程序。Ctrl+C 清理子进程。构建和正常 `cargo run` 使用相同默认 features，不再构建 UI 动态库。CLI 仍通过 PATH 中的 Cargo，沿用 mbx 等包装器。

`capture` 运行应用自己的普通可执行文件，传入 `--scenario <绝对路径> --capture <绝对路径>`。应用需要实现这两个参数；`plico new` 模板和 gallery 已实现。任意应用不一定能截图，未实现该参数的应用应由自身参数解析器报错。`--watch` 会在源文件或场景文件改变后重新生成截图；只有场景 JSON 改动时复用已编译程序，不调用 Cargo。输出宜放在忽略的 `reports/`；生成的目标截图不会触发重新构建。

## JSON 场景

```json
{
  "version": 1,
  "viewport": { "width": 1120, "height": 820, "scale": 1.0 },
  "state": { "count": 4 },
  "steps": [
    { "after_ms": 0, "command": { "action": "click", "role": "button", "label": "Increment" } }
  ],
  "time_ms": 200
}
```

`state` 是应用定义的类型，可以是 App 本身，也可以是构造 App 的专用配置。应用的普通运行接口不再要求 App 实现 Serialize/Deserialize。只有预览配置需要反序列化；线程、SDK、密钥等不放入场景。

`steps` 使用 `plico_runtime::testing::Command`，包括 click、focus、set_value、text、key、key_event。`after_ms` 在该步前推进虚拟时钟；`time_ms` 在最后一步后再推进一次，不睡眠、不模拟中间 vsync 帧。动画需要逐帧采样时，使用多个步骤或 Rust 测试驱动器。重复 label 必须指定 occurrence，找不到目标会报错。

```rust,ignore
let scenario = plico_desktop::PreviewScenario::<PreviewConfig>::load(path)?;
let frame = scenario.render(|config| {
    plico_desktop::NativeApp(MyApp::from_preview(config))
})?;
frame.write_png(output)?;
```

工厂可注入静态服务结果，避开网络、模型加载和真实账单。框架不猜测应用的异步完成条件，也不自动序列化宿主资源。资源路径由应用工厂解释；建议相对于工程目录或由工厂显式解析为场景相对路径。时钟能保证 UI 逻辑时间可重复，但跨机器的字体和 Skia 版本仍可能产生像素差异。

目前正式支持 JSON；尚未引入 RON。离屏预览默认使用 wgpu，也可显式选择 Skia CPU 对照；WGSL shader 由 wgpu 支持。真实窗口继续用于验证输入、剪贴板、IME、系统窗口行为与 GPU 显示。

WASM 的 host/UI 拆分加载是独立能力，保留其值协议与检查点；这不表示桌面动态热重载仍存在。`plico-dev` 当前仍承载 App、服务、Task 和 WASM 值协议，包名暂不随本次迁移更改。

## wgpu 离屏预览

`plico-desktop/wgpu` 现在也提供同一套 `capture` 与 `PreviewScenario`，不需要
开启 Skia。默认 Auto 优先选择 wgpu，仅启用 Skia 时选择 Skia；也可显式指定：

```rust,ignore
let frame = scenario.render_with_backend(
    plico_desktop::RenderBackend::Wgpu,
    plico_desktop::NativeApp,
)?;
frame.write_png(output)?;
```

低层 frame source 使用 `capture(&mut source).renderer(RenderBackend::Wgpu)`；
原生 widget source 可用 `render_source_with_backend`。声明、脚本事件几何和最终
绘制复用同一个后端实例及文字服务，避免预览布局使用另一套文字度量。
显式选择不可用的后端会报错，不静默换成另一个 renderer。此 capture API 目前
只支持 Skia／wgpu。wgpu 已连接 Linux／Windows／macOS 的系统文字；macOS CoreText 接入仍属实验，尚无实机截图验证，富文本、应用字体与截断会明确报错。

Gallery 可直接使用已有场景：

```sh
mkdir -p reports
cargo run --locked -p gallery --no-default-features --features wgpu -- \
  --scenario apps/gallery/scenarios/controls.json \
  --capture reports/gallery-wgpu.png
```

本机已实际生成并检查 Controls 截图，配置计数 4 加一次脚本点击后显示 5。
GPU 测试验证 1.5× DPI 的重复像素、脚本点击与虚拟时钟 210ms；另验证 capture
保持预乘 RGBA，而 PNG 写出为直通 alpha。此前 Skia 配置的七项 capture／preview
回归通过。生成图仍属本地产物，不提交到仓库。

额外的 wgpu GPU 测试覆盖 hosted image canvas 的 1.25× 预览；Windows 仅验证
交叉编译，尚无 Windows 实机截图结论。
