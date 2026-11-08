# Plico 项目约定

- Plico 是后续 Rust GUI 开发的主仓库。原 Wabou 暂停开发；除非用户另行明确要求，不在原 Wabou 仓库实现功能、修复或提交改动。
- 需要复用的 Wabou 代码直接复制到 Plico 后维护，保留必要的许可证和来源记录。Plico 的正常 Rust 构建不依赖原 Wabou checkout，不引入指向它的 path dependency 或源码 symlink。
- 原 Wabou 可以作为只读参考；Solid gallery 对照脚本目前仍可使用其现有 CLI 和前端包。它是可选验证工具，不是 Plico 的开发或运行前提。
- 根目录同时是 Cargo workspace 和不发布的 examples package，用于 examples/ 下可直接运行的最小示例。框架与后端 package 放在 crates/，完整示例应用放在 apps/，crate 专属的小型 API 示例保留在对应 crate 的 examples/；plico 提供不依赖渲染与窗口的 immediate Engine、共享类型和 widgets；plico-runtime 提供宿主无关的声明运行时（adapter、Source、HostView、动效）；wgpu 为默认桌面 renderer，也是画质与性能优化的主要目标；Skia 保留为可选对照，不要求每项特效持续对齐，plico-desktop 是桌面窗口宿主与开发 CLI，plico-cli 提供开发命令。早期 reactive 与 Rune 实验不作为默认路径；当前正式的显式依赖 Regions API 位于 plico-runtime::regions，与 immediate API 共用 Engine 和后端。
- 提交使用 Cocogitto / Conventional Commits，参见 docs/contributing.md。
- 设计目标、原实验结果与当前已实现能力要明确区分，参见 docs/README.md、docs/status.md。

- CLI 参数解析统一使用 usage-rs，不引入 Clap 或自行维护完整参数解析器。错误处理使用 Snafu。

- 新的布局代码可使用 #[component] + #[rustfmt::skip]，以原生 `layout + |cx| { ... };` 声明嵌套 children。#[component] 注入组件和调用点身份；不再支持双分号语法。原生 `.children(|cx| { ... })` 保留为底层 API。专用 formatter 使用 plico fmt（--check 只检查），view! 已移除。

- 根接口按受众分层：App 和 plico::View 面向用户，前者包含应用级状态与开发生命周期，后者是最小 show/theme 声明接口；plico_runtime::HostView 只面向后端和需要宿主能力的高级应用。简单组件继续使用普通 Rust 函数。
- 新应用使用普通 Rust 状态和 plico_dev::App::show；事件触发声明，后端保留节点、文字、布局与绘制缓存。静态节点不手写 key，动态列表使用业务 ID。应用不维护 changed。Regions 使用显式 State 句柄与依赖参数，回调执行时取得短期 Access；允许区域内使用普通 immediate 控件声明。宏仅注入组件和调用点身份，不自动收集依赖或改写闭包捕获。时间从 Clock/FrameTime 注入，不在组件中读取系统 Instant。

- 新的应用代码优先使用 UiContext 首参的独立控件函数（plico::widgets / prelude），自定义组件优先普通函数，不要求实现 Widget trait。两个 Widget trait 分界：plico::Widget（fn show）是应用可组合的 immediate 辅助 trait；plico_renderer::Widget（measure/paint/IME 平台契约）只允许框架 crate（editor、terminal 等）实现，应用可以宿主这类 widget（调用其 trait 方法、接收 WidgetNodeEvent），但不得自行实现或触碰其 MeasureContext/PaintContext 表面；scripts/check-widget-boundary.py 在 CI 中守卫此边界。布局优先使用 `+ |cx| { ... }`，原生 children 保留；动态格式化文字优先 text!(cx, ...)，已有字符串直接借用。异步任务留在宿主，通过服务快照和消息回到 UI，宿主服务不持有 UI 状态引用；Regions 的 UI 依赖由组件生命周期管理。

- reports/ 下的截图、日志与性能报告是本地生成产物，不参与提交；提交可重建脚本、测试断言与文档结论，文档不依赖生成文件链接。

- 开发使用静态编译重启与 JSON 场景预览（docs/preview.md）。桌面动态热重载、stabby ABI 与 cdylib 导出已移除，不重新引入。App 不强制序列化；预览状态采用应用定义的配置类型。WASM host/UI 拆分协议独立保留。

- 可复用的原生桌面能力（托盘、原生菜单、通知回调、窗口生命周期与事件循环唤醒）在 Plico 框架实现，经 plico-desktop 公开；应用只提供品牌资源、菜单与业务事件映射、通知内容和偏好持久化，不在应用重复实现平台后端。不得把密码库、SDK、SSH 审批等 GPL 应用业务搬入框架。

- Rust 控件与布局默认使用 corner_smoothing = 1 的连续圆角；不要要求应用逐个开启。显式 .corner_smoothing(0.) 保留圆弧选择，完全圆角的正圆／胶囊使用精确圆弧。
