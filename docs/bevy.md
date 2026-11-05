# Bevy plugin

`plico-bevy` 将 Plico 作为 Bevy 的 UI 插件。Bevy 管理窗口、GPU、相机、游戏循环和最终呈现；Plico 继续使用原来的 immediate 控件、自动调用点 ID、Taffy、系统文字与 wgpu 绘制。没有启动第二个窗口循环，也没有创建第二个 GPU device。

## 运行

```sh
cargo run -p plico-bevy --example overlay
```

示例是带灯光和阴影的 3D 场景。Plico 的开关控制立方体旋转，滑块调节速度；输入框支持文字编辑和 IME，列表可以滚动。UI 透明部分保留 Bevy 场景。

Bevy 当前版本为 0.20.1，与 Plico 共享 wgpu 30。示例默认优先低功耗适配器，与 Plico 桌面端一致；混合显卡的 Wayland 系统中，默认高性能适配器可能不能向当前 compositor 呈现。可以用 `WGPU_POWER_PREF` 或 `WGPU_ADAPTER_NAME` 显式覆盖。插件本身不替宿主选择显卡。

## 应用 API

```rust,ignore
App::new()
    .add_plugins(DefaultPlugins)
    .add_plugins(PlicoPlugin::<Settings>::default())
    .add_systems(Startup, setup_scene)
    .run();
```

`Settings` 实现普通 `plico::View + Default`。初始化状态可在添加插件前通过 `app.insert_non_send(PlicoUi::new(settings))` 注入。控件函数和 `layout + |cx| { ... }` 写法不变；不需要将控件转成 ECS Entity。

Bevy system 可以通过 `NonSend<PlicoUi<Settings>>` 读取 UI 状态；外部状态变化通过 `NonSendMut<PlicoUi<Settings>>::update` 更新，自动请求声明。借用只在 system 本次调用内存活，不保存对 `World` 或 Bevy resource 的引用。

```rust,ignore
fn sync_progress(progress: Res<Download>, mut ui: NonSendMut<PlicoUi<Settings>>) {
    if progress.is_changed() {
        ui.update(|settings| settings.progress = progress.fraction);
    }
}
```

`PlicoSystems::Input` 在 `PreUpdate` 运行；游戏输入应排在它之后，并检查 `PlicoInput` 的 `wants_pointer`、`wants_keyboard` 和本帧 `*_consumed`。插件不会清空 Bevy 的原始输入。UI 在主世界更新完成后的同步 `ExtractSchedule` 中声明/准备；需要同步业务资源的 system 可放在 `Update` / `PostUpdate`。这避免 Plico 的 queue submission 与 Bevy render thread 的交换链重配并发。

默认 `clipboard` feature 使用 Bevy 的系统剪贴板。禁用该 feature 时，应用可以读取 `PlicoClipboardRequest` 消息并将读取结果作为 `UiEvent::Paste` 回送；异步结果应验证焦点客户端没有改变。窗口/URL 等宿主操作通过 `PlicoHostAction` 消息交给应用执行，不会自动操作宿主窗口。

## 生命周期与绘制

- Plico 状态、文字布局和 renderer 保留在主线程，只向 render world 提取不可变的 GPU 纹理句柄。
- UI 使用 Bevy `Time` 的 delta 推进注入的 `VirtualClock`，遵循游戏时间暂停与倍速。
- 复用 Source 的声明、布局和 Scene 缓存。没有变化的 UI 复用整张纹理，每帧只进行透明合成。
- UI 在同步提取阶段变化时完整更新其透明纹理，**不启用像素级 damage tracking**。
- 纹理池不覆盖仍被已提取帧引用的纹理，避免 Bevy 流水线渲染时混用前后帧。
- 合成排在 Bevy 相机提交之后、截图和 present 之前。使用 source-over，保留游戏像素，处理 sRGB 目标的线性预乘。

## 当前边界与验证

当前接入面向原生窗口，WASM 尚未验收。只支持 primary window 的全窗口覆盖层，需要至少一个输出到该窗口的相机。多个相机共享窗口时只合成一次。不包含世界空间 UI、多窗口路由、完整无障碍平台接入或 GPU device loss 恢复。读取背景的 shader 只能读取 Plico UI 自身的背景，尚不能采样 Bevy 游戏画面。

Linux 上已完成真实窗口交互、175% 缩放与连续 resize 验证，以及独立 GPU 合成像素断言。Windows 编译由 CI 检查；Linux 本机交叉编译缺少 MSVC 汇编工具，不能代替 Windows runner。

本地验证入口：

```sh
cargo test --locked -p plico-bevy --lib
cargo test --locked -p plico-bevy --lib shared_device_overlay -- --ignored
cargo clippy --locked -p plico-bevy --all-targets --no-deps -- -D warnings
```

第二项需要 GPU，不放入常规 CI。示例可通过 `PLICO_BEVY_CAPTURE=reports/bevy/overlay.png` 在启动后自动使用 Bevy 截图；生成产物不提交。
