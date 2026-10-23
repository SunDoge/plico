# 后端清理

当前 workspace 保留 9 个 package：plico、plico-macros、plico-dev、plico-cli、plico-gallery、plico-backend，以及 plico-renderer、plico-shell、plico-motion。

删除 wabou-bindgen、wabou-bindgen-macros、wabou-host-api、wabou-protocol、wabou-style、wabou-telemetry 六个独立 crate。删除旧 JS 绑定生成入口、二进制 UI 协议、utility-class/stylesheet 编译器，以及 shell 的旧 widgets feature 和实现。对应的 Specta、QuickJS、协议 intern 等依赖与无引用 fixtures 一起移除。

仍被原生后端使用的 ResourceKey 位于 renderer::platform；renderer 所需的三个样式 IR 值类型移入 style_ir；CPU 诊断保留在 shell::telemetry。原生资源句柄校验、样式值反序列化和诊断测试随模块保留。Plico 的 Clock/FrameTime 注入保持不变；删除的是旧 widgets 的 SharedClock hook，它没有接入 Plico 的时钟链路。

gallery 是应用而不是后端依赖：main.rs 启动桌面入口，lib.rs 定义同一个 Gallery App 和动态模块导出。desktop 为默认 feature，关闭后仍是轻量 rlib/cdylib；后端只在测试时依赖 gallery。render/probe 的 CLI 路由指向 gallery 的独立验证示例，应用入口不解析参数。

```sh
cargo run -p plico-gallery
cargo plico render --out /tmp/gallery.png
cargo build -p plico-gallery --lib --no-default-features
cargo plico dev
```

渲染、文字、布局、通用 Widget/FrameSource 和平台事件/窗口能力继续保留。后续 widget 在 Plico 声明层实现；本次不重写成熟渲染算法，不修改原 Wabou checkout。历史 Solid 对照代码仍为 workspace 外的可选验证材料。UPSTREAM.json 保存原始来源清单和哈希，并额外记录当前保留 crate 与模块迁移。

原 wabou-shell-api 已整体并入 plico-renderer::platform，事件、窗口、effect 和资源句柄保持同一实现与测试。shell 引用 renderer 的 platform 模块，无独立契约 crate，也不产生循环依赖。
