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

`steps` 使用 `plico_runtime::testing::Command`，包括 click、focus、set_value、text、key、key_event、pointer_move、pointer_leave。`after_ms` 在该步前推进虚拟时钟；`time_ms` 在最后一步后再推进一次，不睡眠、不模拟中间 vsync 帧。动画需要逐帧采样时，使用多个步骤或 Rust 测试驱动器。重复 label 必须指定 occurrence，找不到目标会报错。

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
只支持 Skia／wgpu。wgpu 已连接 Linux／Windows／macOS 的系统文字；macOS CoreText 接入仍属实验，已支持 UTF-8 富文本 span 与单行截断。应用字体注册、span 独立行高和多行省略仍会明确报错。

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

## 指针与视觉效果

指针坐标采用窗口逻辑像素，与鼠标事件一致，不乘 device scale。
`pointer_move` 更新 hover 和组件中的 `cx.pointer_position()`，`pointer_leave` 清除它。
非有限坐标会被拒绝；修改 JSON 后可重复渲染，无需真实移动系统鼠标。

```json
{
  "version": 1,
  "viewport": { "width": 1120, "height": 820, "scale": 1.0 },
  "state": { "black_hole": true },
  "steps": [
    { "command": { "action": "pointer_move", "x": 675, "y": 480 } }
  ],
  "time_ms": 1200
}
```

Gallery 的完整场景为 `apps/gallery/scenarios/black-hole.json`。
`state` 仍是应用定义的预览配置；它不是自动写入任意运行中 Rust 字段的反射接口。
真实窗口的外部 IPC 与运行时状态路径写入尚未实现。

## 编译一次，运行多个 recipe

实验已比较独立进程串行/并行、单进程独立 renderer、单进程共享 renderer 和仅语义断言。
Warden 七个场景的五轮测量及隔离检查见 [Recipe 批处理实验](experiments/recipe-batching.md)。
当前保留现有 JSON `state + steps + time_ms`，不引入新的场景 DSL。

框架提供 `PreviewSession` 复用后端资源。每次 `render_in` 仍调用工厂创建全新 App，
重置 Source、文字上下文、输入状态和虚拟时钟；不沿用上一场景的 UI 状态。

```rust,ignore
let mut session = plico_desktop::PreviewSession::new(plico_desktop::RenderBackend::Wgpu);
for (scenario_path, output_path) in recipes {
    let scenario = plico_desktop::PreviewScenario::<PreviewConfig>::load(scenario_path)?;
    scenario.render_in(&mut session, |config| {
        plico_desktop::NativeApp(MyApp::from_preview(config))
    })?.write_png(output_path)?;
}
```

`PreviewSession` 是有限批次的资源复用 API，不是常驻窗口或进程崩溃隔离机制。
自定义 native surface 可使用 `render_source_in`。Skia 在尺寸变化时重建 surface；
目前批处理实验数据和新回归测试来自 wgpu。通用 `plico capture` 尚未增加批处理参数，
实验命令位于独立 [Plicowarden](plicowarden.md) 的 `recipe_probe` example。

## TypeScript 测试计划

需要 import 公共状态、组合交互、步骤中间断言和多个截图点时，可使用
[TypeScript recipe 开发循环](recipes.md)。TS 生成独立的 `RecipePlan<S>`，
由应用的普通 Rust executable 执行；已有 PreviewScenario 和 capture 命令不变。

```sh
cd ../plicowarden
python3 scripts/run-recipes.py
python3 scripts/run-recipes.py --no-build
```

当前应用命令位于独立 Plicowarden 仓库，已接通其合成 fixture；其他应用可使用
`plico_desktop::recipe::RecipePlan::run` 接自己的工厂和 typed 服务事件处理函数。
