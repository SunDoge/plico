# 场景预览与开发循环

默认开发路径是静态编译、自动重启与确定性离屏截图。桌面动态库加载、stabby ABI、模块状态迁移和对应 feature 已移除。重启会重建应用；需要保留的数据由应用正常持久化。

```sh
cargo plico dev apps/gallery
cargo plico capture apps/gallery --scenario apps/gallery/scenarios/controls.json --output reports/controls.png
cargo plico capture apps/gallery --scenario apps/gallery/scenarios/controls.json --output reports/controls.png --watch
```

`dev` 监听源码，编译成功后关闭旧进程并启动新程序。编译失败保留旧程序。Ctrl+C 清理子进程。构建和正常 `cargo run` 使用相同默认 features，不再构建 UI 动态库。CLI 仍通过 PATH 中的 Cargo，沿用 mbx 等包装器。

`capture` 运行应用自己的普通可执行文件，传入 `--scenario <绝对路径> --capture <绝对路径>`。应用需要实现这两个参数；`plico new` 模板和 gallery 已实现。任意应用不一定能截图，未实现该参数的应用应由自身参数解析器报错。`--watch` 会在源文件或场景文件改变后重新生成截图；只有场景 JSON 改动时复用已编译程序，不调用 Cargo。输出宜放在忽略的 `reports/`；生成的目标截图不会触发重新构建。

直接运行 `plico dev <应用目录> -- --scenario ... --capture ...` 时，这两个路径相对于应用目录解析。原生模式下，修改 WGSL shader 或 Fluent 翻译文件（`.ftl`）也会触发增量构建。原生预览按上一次成功构建的依赖过滤其他 workspace 成员的编辑。跨成员的 `include_str!` 和 `include_bytes!` 资源由 Cargo 的依赖文件识别。Linux 在注册监听前跳过构建目录。

## 保留窗口的 Rust UI 热重载

安装编译目标后，在 Plico 根目录运行阅读器，将最后一个参数换成实际 PNG 的绝对路径。

```sh
rustup target add wasm32-unknown-unknown
cargo plico dev benchmarks/wasm-ui --wasm -- /absolute/path/to/page.png
```

点击 **Analyze image**，然后修改阅读器的 `src/tree.rs`。CLI 会编译 guest 并原子替换模块。宿主在后台准备新版，窗口、图片资源和正在执行的任务继续使用同一进程。guest 的局部状态会随实例重建；需要跨版本保留的值由宿主持有。

接入应用时，在同一 Cargo package 声明宿主和 guest 两个 bin，配置要监听的源码。路径相对于 package 根目录，可列文件或目录。

```toml
[package.metadata.plico.wasm]
host = "reader-host"
guest = "reader-ui"
features = ["reader-service", "tree-patch"]
host-features = ["wasm-host"]
watch = ["src/ui.rs", "src/tree.rs", "src/service.rs", "src/bin/guest.rs"]
```

`features` 用于两次构建。guest 关闭默认 features；宿主保留默认 features，再加上 `host-features`。宿主从 `PLICO_WASM_MODULE` 读取模块路径。guest 用 `plico_wasm::export_ui!` 导出约定版本、缓冲区容量和处理函数；宿主通过 `Module::load` 校验并实例化，`Module::call` 返回借用的响应字节。双方使用 rkyv 的小端、32 位相对偏移和 unaligned 格式。处理函数接收已校验的归档引用；宿主用 `rkyv::access` 原地读取返回字段。借用只在下一次调用之前有效。`Reloads::watch` 在后台加载候选实例，并唤醒宿主应用更新。修改宿主代码、协议或 Cargo 配置后重启开发命令。

发布时编译相同的 Rust UI 并直接调用，应用把 `plico-wasm/host` 放在可选的开发 feature 中。当前模块接口用于受信任的本地开发代码。加载外部插件的产品还需要定义服务权限、执行和内存预算、出错恢复及协议兼容策略。

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

浏览器的 host/UI 拆分路径继续使用现有值协议与检查点。`plico-dev` 当前仍承载 App、服务、Task 和 WASM 值协议，包名暂不随本次迁移更改。

## 程序化录制

`plico record` 重放同一个 `PreviewScenario`，在虚拟时钟上以固定帧率离屏采样。
场景的 `after_ms` 决定精确的输入时刻，最后一步后的 `time_ms` 决定尾部停留时间。
分辨率和 DPI 由 `viewport` 控制，采样范围包括终态。录制复用一个 GPU 和文字会话。

```sh
cargo plico record apps/gallery \
  --scenario apps/gallery/scenarios/liquid-recording.json \
  --output reports/liquid-glass.mp4 --fps 60
```

MP4 输出依赖 PATH 中带 `libx264` 的 FFmpeg。`--fps` 默认 60，支持 1 到 120。
Gallery 与 `plico new` 的应用模板已接入 `--scenario`、`--record`、`--fps` 参数。
脚本或编码失败时保留已有输出。视频使用 H.264 与 4:2:0，奇数像素边缘补一列或一行。

Rust 测试可以用 `PreviewScenario::record_frames` 接收每帧的虚拟时间和原始 RGBA，
直接断言关键帧，或交给自定义编码器。这个接口不需要 FFmpeg。无窗口的录制覆盖 Plico
绘制内容，原生系统菜单等窗口宿主表面仍由真实窗口测试覆盖。带语义断言的 `RecipePlan`
仍从自己的入口执行，`plico record` 接收 PreviewScenario 格式。

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

加 `--frames N` 可重复渲染并在最后一帧读取渲染器预算；`PLICO_FRAME_BUDGET=1`
打印 prepare/upload/encode/submit/wait/GPU、instance、draw 与上传字节，
`PLICO_GPU_TIMER=1` 请求 timestamp query。两者只用于本机测量，字段含义见
[性能](performance.md)。

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

### 按压中间状态

`{"action":"pointer_down","x":340,"y":390}` 与对应的 `pointer_up` 分开派发主指针输入，
可以在两步之间用虚拟时间截取 pressed 状态。`pointer_move` 保留活动控件／滑杆／滚动条捕获。
坐标以窗口逻辑像素表示，非有限坐标拒绝。业务激活仍可用语义 `click`；按压截图不需要提前触发激活。
可参考 Gallery 的 `effects-glass-hover.json` 和 `effects-glass-pressed.json`。
