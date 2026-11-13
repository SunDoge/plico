# Plico 项目约定

- Plico 是后续 Rust GUI 开发的主仓库。原 Wabou 暂停开发；除非用户另行明确要求，不在原 Wabou 仓库实现功能、修复或提交改动。
- 需要复用的 Wabou 代码直接复制到 Plico 后维护，保留必要的许可证和来源记录。Plico 的正常 Rust 构建不依赖原 Wabou checkout，不引入指向它的 path dependency 或源码 symlink。
- 原 Wabou 可以作为只读参考；Solid gallery 对照脚本目前仍可使用其现有 CLI 和前端包。它是可选验证工具，不是 Plico 的开发或运行前提。
- 根目录同时是 Cargo workspace 和不发布的 examples package，用于 examples/ 下可直接运行的最小示例。框架与后端 package 放在 crates/，完整示例应用放在 apps/，crate 专属的小型 API 示例保留在对应 crate 的 examples/；plico 提供不依赖渲染与窗口的 immediate Engine、共享类型和 widgets；plico-runtime 提供宿主无关的声明运行时（adapter、Source、HostView、动效）；wgpu 是唯一主线桌面 renderer，也是画质与性能优化目标；Skia 已进入退役阶段，只保留历史对照，不进入主线 CI、不新增特效或平台能力，plico-desktop 是桌面窗口宿主与开发 CLI，plico-cli 提供开发命令。早期 reactive 与 Rune 实验不作为默认路径；当前正式的显式依赖 Regions API 位于 plico-runtime::regions，与 immediate API 共用 Engine 和后端。
- 提交使用 Cocogitto / Conventional Commits，参见 docs/contributing.md。
- 设计目标、原实验结果与当前已实现能力要明确区分，参见 docs/README.md、docs/status.md。

- CLI 参数解析统一使用 usage-rs，不引入 Clap 或自行维护完整参数解析器。错误处理使用 Snafu。

- 新的布局代码可使用 #[component] + #[rustfmt::skip]，以原生 `layout + |cx| { ... };` 声明嵌套 children。#[component] 注入组件和调用点身份；不再支持双分号语法。原生 `.children(|cx| { ... })` 保留为底层 API。专用 formatter 使用 plico fmt（--check 只检查），view! 已移除。
- 开发中按改动 package 或文件运行 `plico fmt`；准备 commit／push 时，必须在最后一次源码修改之后运行与 CI 完全相同的 `cargo run --locked --profile ci -p plico-cli -- fmt --check`。这条检查之后若又修改 Rust／WGSL，检查结果立即作废，提交前必须重跑；不得把远程 CI 当作格式检查器。

- 根接口按受众分层：App 和 plico::View 面向用户，前者包含应用级状态与开发生命周期，后者是最小 show/theme 声明接口；plico_runtime::HostView 只面向后端和需要宿主能力的高级应用。简单组件继续使用普通 Rust 函数。
- 新应用使用普通 Rust 状态和 plico_dev::App::show；事件触发声明，后端保留节点、文字、布局与绘制缓存。静态节点不手写 key，动态列表使用业务 ID。应用不维护 changed。宏仅注入组件和调用点身份，不自动收集依赖或改写闭包捕获。时间从 Clock/FrameTime 注入，不在组件中读取系统 Instant。

- 新的应用代码优先使用 UiContext 首参的独立控件函数（plico::widgets / prelude），自定义组件优先普通函数，不要求实现 Widget trait。两个 Widget trait 分界：plico::Widget（fn show）是应用可组合的 immediate 辅助 trait；plico_renderer::Widget（measure/paint/IME 平台契约）只允许框架 crate（editor、terminal 等）实现，应用可以宿主这类 widget（调用其 trait 方法、接收 WidgetNodeEvent），但不得自行实现或触碰其 MeasureContext/PaintContext 表面；scripts/check-widget-boundary.py 在 CI 中守卫此边界。布局优先使用 `+ |cx| { ... }`，原生 children 保留；动态格式化文字优先 text!(cx, ...)，已有字符串直接借用。异步任务留在宿主，通过服务快照和消息回到 UI，宿主服务不持有 UI 状态引用。

- reports/ 下的截图、日志与性能报告是本地生成产物，不参与提交；提交可重建脚本、测试断言与文档结论，文档不依赖生成文件链接。

- 开发使用静态编译重启与 JSON 场景预览（docs/preview.md）。桌面动态热重载、stabby ABI 与 cdylib 导出已移除，不重新引入。App 不强制序列化；预览状态采用应用定义的配置类型。WASM host/UI 拆分协议独立保留。

- 可复用的原生桌面能力（托盘、原生菜单、通知回调、窗口生命周期与事件循环唤醒）在 Plico 框架实现，经 plico-desktop 公开；应用只提供品牌资源、菜单与业务事件映射、通知内容和偏好持久化，不在应用重复实现平台后端。不得把密码库、SDK、SSH 审批等 GPL 应用业务搬入框架。

- Rust 控件与布局默认使用 corner_smoothing = 1 的连续圆角；不要要求应用逐个开启。显式 .corner_smoothing(0.) 保留圆弧选择，完全圆角的正圆／胶囊使用精确圆弧。

- 当前优先支持纯 Rust 路径；新渲染架构和公共 API 不以 FFI/C# ABI 兼容或跨语言验证作为交付前置。除非用户明确要求，不新增跨语言适配层。

- 遇到界面或桌面宿主问题，先查 MyGO 对应源码，再决定修复。控件交互、动效、玻璃合成、阴影和渲染缓存优先对照 MyGO 的现有实现，再决定 Plico 的适配；不要在已有成熟参考时从头设计。可直接迁移的代码保留许可证与来源 revision，通用能力沉淀到框架，以确定性 recipe 和相关性能断言验证。MyGO checkout 只用于参考，不成为构建依赖。

- MyGO 对齐工作把对应源码视为规格：优先逐项翻译组件结构、精确尺寸、状态、绘制顺序、交互和动效，不凭截图重新设计，也不为追求 Rust 风格另造一套近似 API。若 Plico 缺少表达该实现的通用能力，先在框架补一次可组合的基础 API，再翻译组件；不得用应用层特例绕过。
- MyGO 组件按完整组件族批量迁移。实现阶段以 Rust 单元／集成测试验证结构、状态和交互，整批完成后再做一次统一视觉验收；不要为每个微小样式反复进行双语言构建和截图。性能优化与源码对齐分轮进行：先得到行为和视觉基线，再用测量结果做 Rust／wgpu 专属优化，优化不得改变已锁定的组件契约。
