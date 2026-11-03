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

目前正式支持 JSON；尚未引入 RON。离屏路径使用 Skia CPU，WGSL shader 仍只在 Vello 路径支持。真实窗口继续用于验证输入、剪贴板、IME、系统窗口行为与 GPU 显示。

WASM 的 host/UI 拆分加载是独立能力，保留其值协议与检查点；这不表示桌面动态热重载仍存在。`plico-dev` 当前仍承载 App、服务、Task 和 WASM 值协议，包名暂不随本次迁移更改。
