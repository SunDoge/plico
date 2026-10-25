# Plico 项目约定

- Plico 是后续 Rust GUI 开发的主仓库。原 Wabou 暂停开发；除非用户另行明确要求，不在原 Wabou 仓库实现功能、修复或提交改动。
- 需要复用的 Wabou 代码直接复制到 Plico 后维护，保留必要的许可证和来源记录。Plico 的正常 Rust 构建不依赖原 Wabou checkout，不引入指向它的 path dependency 或源码 symlink。
- 原 Wabou 可以作为只读参考；Solid gallery 对照脚本目前仍可使用其现有 CLI 和前端包。它是可选验证工具，不是 Plico 的开发或运行前提。
- 根目录是虚拟 Cargo workspace。框架与后端 package 放在 crates/，完整示例应用放在 apps/，小型 API 示例保留在对应 crate 的 examples/；plico-core 提供不依赖渲染与窗口的 immediate Engine、共享类型和 widgets；plico-runtime 提供宿主无关的声明运行时（adapter、Source、View、动效）；plico-vello 是桌面窗口宿主与开发 CLI，plico-cli 提供开发命令。reactive 与 Rune 实验已删除，不重新引入为默认路径。
- 提交使用 Cocogitto / Conventional Commits，参见 docs/contributing.md。
- 设计目标、原实验结果与当前已实现能力要明确区分，参见 docs/README.md、docs/status.md。

- CLI 参数解析统一使用 usage-rs，不引入 Clap 或自行维护完整参数解析器。错误处理使用 Snafu。

- 新的布局代码可使用 #[component] + #[rustfmt::skip]，以原生 `layout + |cx| { ... };` 声明嵌套 children。#[component] 注入组件和调用点身份；不再支持双分号语法。原生 `.children(|cx| { ... })` 保留为底层 API。专用 formatter 使用 plico fmt（--check 只检查），view! 已移除。

- 新应用使用普通 Rust 状态和 plico_dev::App::show；事件触发声明，后端保留节点、文字、布局与绘制缓存。静态节点不手写 key，动态列表使用业务 ID。应用不维护 changed。时间从 Clock/FrameTime 注入，不在组件中读取系统 Instant。

- 新的应用代码优先使用 UiContext 首参的独立控件函数（plico_core::widgets / prelude），自定义组件优先普通函数，不要求实现 Widget trait。两个 Widget trait 分界：plico_core::Widget（fn show）是应用可组合的 immediate 辅助 trait；plico_renderer::Widget（measure/paint/IME 平台契约）只允许框架 crate（editor、terminal 等）实现，应用可以宿主这类 widget（调用其 trait 方法、接收 WidgetNodeEvent），但不得自行实现或触碰其 MeasureContext/PaintContext 表面；scripts/check-widget-boundary.py 在 CI 中守卫此边界。布局优先使用 `+ |cx| { ... }`，原生 children 保留；动态格式化文字优先 text!(cx, ...)，已有字符串直接借用。异步任务留在宿主，通过服务快照和消息回到 UI，不引入常驻业务订阅图。

- reports/ 下的截图、日志与性能报告是本地生成产物，不参与提交；提交可重建脚本、测试断言与文档结论，文档不依赖生成文件链接。
