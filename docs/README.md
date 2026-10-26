# Plico 设计文档

这些文档记录当前实现、已验证的实验与后续设计方向，不把设计目标当作已完成的功能。

| 文档 | 内容 |
|---|---|
| [MyGo 对标方向](mygo-benchmark.md) | 完整界面的验收标准与下一条应用切片 |
| [Launcher 与命令面板](launcher.md) | 搜索导航、自动聚焦、列表定位、原生窗口动作与演示边界 |
| [2048 对照](2048.md) | MyGo 游戏切片、棋子身份、时钟动画、键盘与模块恢复 |
| [Viewport 与缩放](viewport.md) | 宿主尺寸输入、响应式布局、百分比宽度与新桌面 zoom 示例 |
| [System-wide hotkeys](global-hotkeys.md) | 可选全局热键、平台注册、Wayland portal 与宿主生命周期 |
| [Window-local shortcuts](shortcuts.md) | 完整组合键、作用域 action 表、输入法优先级与热重载事件传递 |
| [Rust 测试协议](testing.md) | 共享语义命令、虚拟时钟、离屏应用测试与窗口验收边界 |
| [Route enum 元信息](router.md) | Route derive、静态导航列表、标签、图标与稳定逻辑 ID |
| [Gallery 与路由](gallery.md) | 页面状态、flex 展台、基础 motion 与 SVG 边界 |
| [动机与开发体验](vision.md) | 为什么选择 immediate API、预期优势和实际代价 |
| [后端清理](backend-cleanup.md) | 当前 crate 划分、删除的旧协议与保留边界 |
| [Crate 命名与迁移](crate-names.md) | Vello 后端、Web host 与 Web demo 的新名称及外部应用迁移 |
| [架构与后端边界](architecture.md) | 节点身份、保留存储、声明协调、Wabou 与 Bevy |
| [FLTK + Taffy 后端实验](fltk.md) | 相同设置页、原生控件复用、输入回传与 Linux 开销对照 |
| [LVGL 最小后端实验](lvgl.md) | LVGL 原生控件、相同声明 API、对象复用与点击回传 |
| [同步 local state 与宿主任务](local-state-and-tasks.md) | 组件局部状态、按需重建、宿主任务取消与迟到结果隔离 |
| [状态、事件与提交](runtime.md) | 同步事件、稳定化、失效传播、卸载与错误边界 |
| [缓存与输出路径](cache.md) | 共享 Scene、原生保留路径、克隆计数与 CPU 对比 |
| [滚动声明分离](scroll-presentation.md) | 普通滚动跳过 show、按容器观察和模拟时钟对比 |
| [声明帧与保留帧](frame-scheduling.md) | 原生失效调度、宿主动画、干净帧复用和外部状态更新契约 |
| [声明成本与 memo 实验](memo-experiments.md) | 核心声明、Markdown 解析缓存、宿主动画与更新边界的实验和限制 |
| [增量执行](incremental.md) | dirty journal、活跃动画、paint patch 与场景复用边界 |
| [Scene 编码对照实验](scene-encoding-benchmark.md) | 增量编码移除依据、重声明成本与完整编码基准 |
| [Skia / Vello 绘制对照](skia.md) | SkParagraph 普通文字、CPU / Vulkan Skia，以及相同 Parley glyph 的后端对照 |
| [Pango/Cairo 文字对照](pango-text-comparison.md) | 排版指标、相同 Parley glyph 的绘制对照与文字清晰度排查 |
| [文字渲染排查结论](text-rendering-investigation.md) | 精确基线、Chrome 对照、字体回退与覆盖率策略的证据及限制 |
| [Native 属性更新实验](native-property-work.md) | 复用布局样式、桌面 CPU 对照与收益限制 |
| [绘制 motion 与失效控制](presentation-motion.md) | transform、命中/语义几何、逐节点 bitflags 与工作量 |
| [布局 API](layout-api.md) | 统一尺寸/四边边距、Flex 扩展、Grid 轨道与命名区域 |
| [布局与样式动画](layout-style.md) | 目标声明、真实重排、属性过渡与模拟时钟验证 |
| [应用内毛玻璃](glass.md) | 背景高斯模糊、底色透明度、圆角裁剪与前缀重放边界 |
| [主题转场](theme-transition.md) | 场景复用与 GPU 圆形裁剪 |
| [宿主服务与可重载 UI](host-services.md) | 类型化快照/消息、常驻 worker、自定义 main 与恢复边界 |
| [热重载协议 v2](hotreload-protocol.md) | 保留会话、checkpoint、输入回传、失败恢复与诊断边界 |
| [动画、异步与热更新](lifecycle.md) | 模拟时钟实验的结论和未来迁移约束 |
| [自动身份](identity.md) | component/view 的隐式 ID、列表 key 与恢复边界 |
| [逻辑时钟](clock.md) | 外部注入、virtual clock、帧时间与动画请求 |
| [Context 与独立组件](components.md) | Context 首参、普通函数组件、行为基础与主题继承 |
| [交互区域 API（设计）](interaction-api.md) | hover 订阅、局部坐标、命中与反馈时序；尚未实现 |
| [SVG 图标](icons.md) | CLI 按需生成 Lucide、应用自有集合、currentColor 与默认勾选标记 |
| [Widget / shader 验收](widget-shader-validation.md) | 本轮目标、实现与逐项验证证据 |
| [Slider 与数值事件](sliders.md) | 受控标量、拖动、键盘、无障碍与热重载 |
| [Custom shader](shaders.md) | WGSL surface、uniform、GPU 资源与宿主时钟 |
| [Tooltip](tooltip.md) | 宿主时钟、键盘提示、描述、指针穿透与重载 |
| [Select / Menu](select-menu.md) | 受控选择、业务 key、命令导航与原生语义 |
| [Popover](popover.md) | 锚点定位、边缘避让、非模态焦点与重载 |
| [Modal 与浮层](modal.md) | 窗口 portal、焦点限制、事件隔离、退出动画和重载 |
| [原生输入](text-input.md) | 单行编辑、密码存储、IME 与静态宿主边界 |
| [代码编辑器](code-editor.md) | Floem 编辑核心、Parley 可见行、native surface 示例与接入边界 |
| [流式 Markdown](markdown.md) | 富文本范围、开放块解析缓存、逻辑时钟展示与 gallery 聊天页 |
| [动态文字格式化](text-formatting.md) | text!、复用缓冲与分配计数边界 |
| [原生操作符 children 实验](experiments/operator-children.md) | + 闭包的编译、LSP、身份、借用与 formatter 验证 |
| [API、宏与格式化](authoring.md) | chain、component、LSP、rustfmt 与专用 formatter |
| [实现状态与验证](status.md) | 已迁入代码、实验来源、测试、性能结论的适用范围 |
| [开发 CLI](cli.md) | dev、render、probe 与 workspace 检查命令 |
| [提交规范](contributing.md) | Cocogitto、Conventional Commits 与本地 hook 安装 |
| [路线图](roadmap.md) | 后续工作的顺序与验收条件 |

阅读顺序建议：vision → architecture → runtime → status。实现变更时同步更新对应文档，尤其要保留“已实现 / 原实验已验证 / 尚在设计”的区别。

- [固定行高虚拟列表](virtual-list.md)：可见窗口、业务身份和热重载滚动反馈。

- [性能与结构同步](performance.md)：CPU/分配探针、局部同步与服务快照版本。

- [批次与唤醒](batching.md)：服务通知合并、输入边界、稳定化提交与计数器。

- [WASM / 浏览器](web.md)：WebGL2 gallery、rAF 宿主、输入桥接与当前范围。

- [原生依赖](native-dependencies.md)：已清理项、仍有用途的依赖与 capability feature 候选。
- [Terminal](terminal.md)：独立 PTY widget、Plico 示例与原生验证。
- [窗口圆角](window-corners.md)：launcher/custom headerbar 的透明 client-area 圆角。

- [原生 children 与组件格式化实验](experiments/component-format.md)：原生 children 的编译、身份、LSP 与 rustfmt skip 验证。

- [隐式上下文箭头语法 LSP 实验](experiments/arrow-lsp.md)：单层宏中 `=> {}` 的补全、悬停、定义跳转矩阵及解析边界。

- [双分号 children 实验（历史）](experiments/semicolon-children.md)：已由原生 operator children 替代，不代表当前支持。

- [无标记尾随块实验](experiments/brace-children.md)：分别验证属性与外层宏的编译边界，以及真实 LSP 行为。

- [单层 view 与局部格式化](single-view.md)：已移除宏的历史记录与路线选择。

- [trailing_closure 展开器评审](experiments/trailing-closure-review.md)：用户提供实现的 AST/span 机制、逻辑或冲突与 GUI API 边界。

- [Native window zoom](window-zoom.md): host scaling, input/IME projection and gallery verification example.

## Optional native services

[Linux native text](native-text.md): Pango/Cairo layout, physical-pixel bitmaps, bounded caching and GPU comparison. Opt-in; ordinary text widgets still use Parley/Vello.

## Widget builder lifetimes

[Lifetime tradeoffs](widget-builder-lifetimes.md): why configurable builders keep separate context/engine borrows, and the rejected short-context performance experiment.

## Widget capabilities

[Input capabilities and backend support](widget-capabilities.md): static input flags, explicit native projection, type-change remounting and declaration-cost validation.
