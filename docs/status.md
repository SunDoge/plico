> 当前开发路径已迁移：桌面热重载、动态 ABI 与模块加载器已移除。使用 [JSON 场景预览与编译重启](preview.md)。下文旧实验中的热重载结论仅作历史记录。

# 实现状态与边界

## 框架材质

`plico::materials::LiquidGlass` 和 `Elevation` 已提供声明式液态玻璃、接触/环境双层阴影预设；
容器自动定位材质背景，内容保持普通控件。Warden 与 Gallery 已消费公共 API，应用不维护玻璃 WGSL。
框架测试覆盖参数、关闭材质、reduced motion、paint-only 更新和 wgpu 折射像素。
启用玻璃当前需要 wgpu；关闭玻璃的纯色替代不依赖 shader。wgpu 已有受最大 sigma 限制的面积预滤波模糊金字塔；Skia 材质与原生系统玻璃尚未实现。
背景采样材质合成继承祖先矩形裁剪与圆角遮罩；滚动、透明分组和裁剪离屏目标均有 GPU 像素回归，覆盖 100%／150%／200% 缩放。
wgpu 已按背景依赖局部失效，并将同阶段等尺寸副本合并为有界纹理数组，批量生成 mip；
全量／局部与独立／分组的逐像素对照及同进程 A/B 见 [背景架构实验](experiments/wgpu-backdrop-architecture.md)。
见 [Liquid Glass](liquid-glass.md)。

## 原生桌面服务

可选 `plico-desktop/desktop-integration` 提供声明式托盘菜单、菜单事件、
关闭后隐藏／恢复、Linux StatusNotifier 宿主探测和消失恢复、可点击系统通知，
通过 `run_source_with_extensions` 接入同一原生事件循环。
普通 NotificationShow effect 与可点击通知共享系统通知构造逻辑。
通知监听最多一个；应用拥有通知内容、偏好存储与业务动作。
已有独立桌面示例和框架单元测试；Linux X11/D-Bus 原生路径由独立消费应用回归。
macOS/Windows 原生与打包身份验证尚未完成，不包含开机启动、系统锁屏或账户服务。
详见 [桌面集成](desktop-integration.md)。

## 窗口状态、激活与文件监视

`WindowOptions` 支持初始位置、最大化和无边框全屏，`WindowCommand::SetFullscreen` /
`ToggleFullscreen` 可在运行时切换，`WindowMetrics.fullscreen` 向应用报告当前状态。
`WindowStatePersistence` 保存普通尺寸、外框位置以及最大化／全屏标志，关闭和退出时原子写入；
旧的仅含尺寸的 JSON 仍然兼容。`run_with_persistence` 默认在应用数据目录恢复
`window-state.json`。

剪贴板在原有文本 effect 之外增加 `ClipboardReadContent` / `ClipboardWriteContent`：
读取时返回可用的文本、HTML、RGBA 图像与文件列表；写入按文件列表、图像、HTML（带纯文本
回退）、文本的顺序选择平台能承载的最丰富表示。

`FileWatcher` 封装 notify 的平台后端，在通知线程中入队并通过 `WakeCallback` 唤醒事件循环；
应用调用 `drain` 消费创建、修改、删除、重命名事件，不轮询文件系统。

`AppActivation` 从启动参数提取已声明的 URL scheme 与文件关联参数，
`SingleInstance::acquire` 使用按用户隔离的本地套接字：首个进程持有监听并在后台接收，
后续进程转发自己的激活后返回 `Secondary`，由宿主决定退出。URL scheme／文件关联的
系统注册、原生应用菜单栏和原生右键菜单执行器仍未实现。

## 原生显示偏好

macOS 监听 NSWorkspace 辅助功能偏好，Linux 在后台共享 portal Settings 连接并监听变化。
减少动画和高对比度接入共享 WindowMetrics / DisplayPreferences，应用不保存系统偏好；
Linux 还兼容 portal 暴露的 GNOME 动画、主题与高对比度设置。没有绘制期系统查询。
Linux 协议与关闭生命周期经私有 D-Bus mock 服务验证；真实桌面后端、异步首帧、
portal 不可用及未实现选项的边界见 [Linux 对照](linux-mygo.md)。

## 默认圆角

Rust Style 的默认 corner_smoothing 已设为 1，控件与布局统一采用连续圆角，半径数值不变。
显式 0 保留圆弧；正圆与完全圆角胶囊在 CPU 与玻璃 WGSL 均保持精确圆弧，不变成方圆形。
默认／serde／显式覆盖、几何与 GPU 圆角和裁剪回归已覆盖。

## MyGO 材质继续迁移

新增 `LiquidGlass::regular()` / `clear()`、`GlassStyle` 与 Clarity。Regular 的明暗 tone
随 theme surface 变化，大面板清晰度不会削弱小控件；旧 default/Custom 保持兼容。
禁用玻璃按钮改用主题 fallback，减少无交互背景采样并保持可读性。
Gallery Effects 提供三种模式、清晰度和控件示例；材质参数、GPU 像素与交互测试已覆盖。
交互玻璃的按压外扩／悬停过渡已接入通用 shader scalar transition 与 paint_outset；原生输入、GPU 像素、焦点、裁剪和 idle 有回归。
新增 `.inset_shadow(BoxShadow)`，在背景上绘制模糊孔洞的补集，并沿连续圆角和祖先 clip 限制范围。
wgpu 内外阴影现已沿连续 caster／孔洞求 Gaussian coverage，显式圆弧和正圆／胶囊继续保留。
CPU 轮廓积分、1/1.5/2 倍缩放、中间平滑度、零 blur、裁剪与 damage 有 GPU 回归。
玻璃与 ScrollEdge 已接入面积预滤波金字塔＋残余 Gaussian 核，减少大半径稀疏采样混叠；
层级按声明的最大 sigma 与 DPI 截止，不保留历史背景纹理，新增颜色／透明度／奇数尺寸／裁剪／LOD 连续性回归。
后续特效质量与性能验证以 wgpu 为主，Skia 为可选对照。

## C# MyGO 改进的 Rust 入口

已补齐 progressive BackdropBlur / 四方向 ScrollEdge、前景 `.blur` / `.no_shadow`、
Tone 与 Info 语义色、chip/tag、有界分页、alert/notification、empty_state 和 native-clock skeleton。
Rust Gallery 新增 Component kit 与圆角对照页；玻璃和 opaque fallback 继承容器的连续圆角。
原本位于 shared Rust 后端的文字、滚动、阴影和缓存优化继续共用，未重复实现。
详细迁移矩阵与边界见 [迁移记录](experiments/csharp-mygo-rust-migration.md)。

## 当前主线

plico 的 Engine / UiContext / widgets 与 plico-desktop 的真实后端继续共用；
新工作、生成模板和 backend-settings 统一采用 Immediate `App::show`。类型安全的
`Router::view().show(...)` 统一稳定页面身份、有限保活和转场，但不引入第二套状态模型。Regions/Reactive
机制、宿主 replay hooks、测试与示例已经删除，不保留第二套应用状态模型。旧 crates/plico 响应式 Runtime、
图实验、共享响应式 2048 组件与 archive-gallery / game2048 / task-workspace 旧应用
已删除。Rune 实验同样已移除。当前 2048 功能在主 Gallery 中维护。

声明层 scalar motion 已对照 MyGO channel 模型接入 `animate`、`animate_with`、
`animate_to`、`spring`、`jump`、`velocity`、`settled` 和 `loop_progress`。channel 按当前
元素、组件 scope 与 key 保存，卸载释放；curve→spring 与拖拽→spring 保留速度，支持
delay、解析弹簧、reduced-motion 和 non-spatial 例外。Gallery sidebar、Motion spring
与液态搜索已迁移，不再由应用保存 `plico_motion::Animated`。

`collapsible` / `accordion` 复用同一 scalar channel 与稀疏布局尺寸反馈：展开面板先透明测量
真实内容高度，再以 200ms ease-out 裁剪；关闭期间保留节点但禁用交互并从语义树隐藏，结束后卸载。
reduced motion 直接挂载／卸载，不增加应用侧动画状态。

`number_input` 复用原生编辑器的 opt-in navigation 通道处理 Up/Down，焦点边沿由声明态记录，
因此离开输入框时可恢复／格式化暂存文字；按钮和键盘共用按 step 小数位取整的有界更新路径。

只读数值组件通过组合容器上的共享 `semantic_range` 写入名称和 `NumericRange`；`meter` 与
`progress` 因而向原生无障碍桥暴露 role、min/max/value，而视觉仍由普通 Rust children 组合。

原生输入的 find-navigation 模式把 Escape、Shift+Enter、Primary+G 与 F3 转成声明响应；
`find_bar` 复用它完成关闭和双向循环跳转，首次自动聚焦时全选旧查询，匹配计数使用 status 角色。

区间滑块保留共享轨道绘制，但结构上是命名 group 加 minimum/maximum 两个真实数值控件；
两个 thumb 分别参与 Tab、键盘和无障碍数值操作，轨道按最近距离选择并聚焦对应 thumb。

`plico-app-host` 单独承载 `plico-dev::App` / services 到 `HostView` 的适配；
`plico-runtime` 的正常依赖图不再包含开发协议层，desktop/web 按需依赖并重导出 bridge。

应用直接持有普通 Rust 状态；局部状态使用 `UiContext::state`，动态列表使用业务 key，路由状态由 `Router` 和应用字段拥有。Engine 的 dirty journal、布局/绘制缓存、虚拟化和 damage 继续承担增量工作，不向应用暴露依赖图或区域重放协议。

## 已实现

- 普通 Rust 函数、调用点身份、动态业务 key、受控输入和事件稳定化。
- 保留节点、属性变化 journal、节点/结构删除与布局/绘制/语义失效。
- Immediate dirty journal 贯通布局、flatten 与 Scene 失效；mount/unmount/reorder 由稳定 Entity 和业务 key 保持身份，大范围结构变化保守回退全量路径。
- 语义 `Arc` 快照与节点自身记录缓存、placed preorder 的 hit-testing 子树包围盒剔除；动态 modal/tooltip/portal 上下文仍逐次计算，ButtonBase/ComboBox 不进入节点缓存。Skia 窗口默认开启局部 damage，原生消费 Fragment；Vello/Web 和全量 Skia redraw 保留展开路径。
- 声明父子关系使用引擎级连续日志，复用节点 children 容量；不为每个节点保留第二份 children Vec。`Engine::trim_declaration_buffers()` 可显式释放声明日志、格式化暂存和 children 多余容量，不改变 Entity 或清除待处理事件。
- `plico` 的 allocations 示例验证 2 万节点场景第二次声明、预热后的无变化/颜色变化/同容量文本变化均无分配；这只覆盖核心声明路径，不代表整个应用或后端零分配。
- Taffy 布局；Vello 普通文字使用 Parley，Skia 普通文字使用 SkParagraph；字体、选择复制、输入法和 SVG。
- 普通文字默认不额外几何加粗，保留 hint 与真实字重，避免淡边；显式扩张按实际扩张量收紧轮廓容差，避免直边变斜。Linux 默认优先 Noto 中英文字体栈，保留应用字体覆盖能力。
- hinted 字形在轮廓缓存插入时精细离散化，减小小字号曲线的局部粗细误差；当前通过本地 Glifo 补丁实现，无 hint 与位图字体不受影响，仍存在亚像素采样导致的局部覆盖率变化。
- Gallery 的布局、控件、动画、2048、[Mines 扫雷](../apps/gallery/docs/mines.md)、流式 Markdown 与应用内毛玻璃。
- Vello 样式按钮的宿主交互权重：hover/press 颜色、焦点环和禁用透明度；支持打断、reduced motion 与闲时停帧。
- [通用声明期裁剪](visibility.md)：VisibleRegion 的矩形相交、保守逆变换与固定行列索引范围；固定行虚拟列表复用该计算。可变行列表另有按需尺寸观察、MyGO 式稀疏测量缓存、业务 key 锚点平移与测量修正，不为普通节点订阅几何。
- 宿主直接绘制 retained 状态，干净帧和纯表现帧不必导出全量节点。
- 普通 main.rs + lib.rs、静态编译重启、JSON 场景离屏截图与宿主服务桥。
- [窗口内快捷键](shortcuts.md)：App/ServiceApp 完整组合键、显式作用域 action 表、冲突检查，以及静态/热重载一致的事件转发。
- [可选系统全局热键](global-hotkeys.md)：plico-native 管理注册与状态，shell extension 负责唤醒和注销。X11 后台触发/冲突/释放已实测，Windows/macOS 通过交叉编译；Wayland portal 已接入并编译，但真实桌面授权与 activation token 聚焦尚未验收。
- usage-rs CLI、plico fmt、应用目录选择与 dev/run 分离。
- plico icons add/check：应用拥有选择集合、生成 Rust、清单和许可证，核心仅内置四个控件图标。
- launcher、terminal、services-demo；编辑器和 WASM 入口保留。

- [Skia / Vello 对照实验](skia.md)：SkParagraph 普通文字、CPU / Vulkan Skia 回放和实验性 Vulkan 窗口；保留相同 Parley glyph 的多缩放对照。

## 同步组件的 local state 与宿主任务

`UiContext::state` 按父节点、组件作用域与调用位置保存局部状态；读写 handle 不借用
UiContext，变化触发现有 settle 流程，卸载释放。普通 state 在进程重启时重置；
persisted_state 使用显式名称，跨卸载保留；宿主可导入/导出
MessagePack 字节实现磁盘或浏览器存储，不默认自动落盘。
`plico_dev::tasks::Task` 封装宿主 Future 的结果、唤醒、替换与取消；执行器及服务快照
仍由应用提供，不让 Future 进入 UI 状态。TaskService 提供单任务服务协议，
UI 可用 services.task().status() 匹配状态并发送 start/cancel/reset。见 [说明](local-state-and-tasks.md)。

## 当前代价和限制

一次业务声明仍会执行视图函数、匹配节点并扫描提交状态；增量后端不等于整条链路 O(Δ)。
依赖布局约束的变化可能扩大重排范围。图标、文字、场景等缓存分别由对应层管理。
布局、Scene、语义和 hit-testing 已有多层缓存，跨层失效规则是当前主要维护风险；typing、reverse 和部分 Skia 动画仍有实测回退。不计划增加 Region 独立布局求解；继续扩大缓存前应先补充全量差分和真实窗口验收。
WASM、分离 host/UI 是已存在的入口和实验，不等于浏览器端编译 Rust。
[LVGL 后端实验](lvgl.md) 使用 lightvgl-sys，提供原生控件映射、对象复用与事件回传，
不是完整后端。ESP32-P4 / ESP-IDF 已有上板验证，范围见 [P4 实验记录](p4-counter-measurements.md)。
Bevy 已有实验性 primary window 覆盖层插件，见 [Bevy 接入](bevy.md)；FLTK 保留实验性适配。尚未支持 no_std，也不承诺其他 ESP32 型号。

## 验证入口

```sh
cargo check --workspace --all-targets
cargo test --workspace --all-features --exclude plico-lvgl --exclude lvgl-esp32p4
cargo run -p gallery
cargo plico new /tmp/my-app
cargo plico dev /tmp/my-app --no-watch
cargo plico icons check --output apps/gallery/src/icons.rs
```

具体行为和局限见 [文档索引](README.md)、[声明模型](declaration-model-choice.md)、
[帧调度](frame-scheduling.md)、[增量更新](incremental.md)、[缓存](cache.md)、
[宿主服务](host-services.md)、[历史热重载实验](hotreload-protocol.md)。
测试、截图和性能数据只代表对应版本与配置；reports 下的本地产物不提交。

## 本次清理验证（2026-10-23）

删除实验后，workspace all-features 测试共 606 项通过，workspace all-targets 编译通过。
通过 plico new 创建的独立项目已完成静态入口编译和 headless 场景执行，
点击 Increment 后业务状态从 0 变为 1，后续帧保持 1。
依赖锁文件已清除 Rune、reactive_graph、Sycamore 及仅供旧实验使用的依赖。

[FLTK + Taffy 后端实验](fltk.md) 已在 Linux/X11 验证共享设置页、原生输入/按钮回调、嵌套卸载、滚动与 keyed 重排。它是独立的可选实验；样式、窗口功能、开发重启及其他平台尚未达到默认 wgpu 后端的覆盖范围。

FLTK 共享工作台现包含 Router 前进/后退、响应式 Grid、固定表头滚动列表，以及基于注入时钟的侧栏/详情栏/任务进度动画。FLTK 宿主按 ANIMATE 请求调度有限帧，结束后阻塞等待事件；尚未自动支持全部 `.motion(...)` 样式插值，见 [实验范围](fltk.md)。

## Optional native text

Implemented in `plico-native` behind Linux `linux-text`: Pango/Cairo layout and rasterization, system font listing, metrics, bitmap caching and GPU comparison tests. See [native text](native-text.md). The experimental wgpu host now uses native ordinary text and single-line editing geometry. Desktop font settings and CoreText are not implemented. Optional `windows-text`
now provides a DirectWrite layout/editing service with cross-compiled tests;
raw grayscale glyph-run masks now cross-compile as well. CPU coverage correction/colorization now follows the MIT-licensed Microsoft
Terminal algorithm with portable reference tests. wgpu integration, color glyphs
and Windows runtime validation remain pending.

## Widget input capabilities

Implemented static `Kind::capabilities()` without per-node storage. Focus, Tab navigation, activation, text selection/editing and numeric editing are independent. LVGL rejects unsupported controls before mutation and remounts native subtrees on class changes. See [capability contracts and validation](widget-capabilities.md). The unused coarse `KindFamily` compatibility API has been removed.

## Damage tracking

Skia windows enable command-stream damage and per-image history by default;
`PLICO_SKIA_DAMAGE=0` restores full drawing. Multi-rectangle damage and Vulkan
incremental-present hints remain opt-in. Tracked partial
redraws consume retained Scene commands and cull unchanged damage regions;
full redraws and Vello/Web retain the expanded replay path. Region-level damage is
conservative rather than a minimal per-leaf rectangle. Normal windows skip an exactly
unchanged presented Scene by default (`PLICO_SKIA_PRESENT_SKIP=0` disables this);
the tracker expands partially intersected text, stroke and atomic Fragment commands
to their complete bounds;
the strict Immediate 240-frame settings probe and repeated-input raster matrices
match full redraws exactly, at a measured coverage/raster cost. Nested Fragment AA
also passes CPU and Intel Vulkan pixel comparisons including resize. Explicitly
supplied damaged regions remain the caller's responsibility, and the
reduced dirty-clip probe stays an ignored diagnostic. Vello's preserved-target partial
clear probe passes, while its normal window renderer remains unchanged. See
[measurements and limits](performance.md).

## 国际化

`plico-i18n` 提供 Fluent 消息和裁剪后的 ICU4X 数字/日期数据；核心通过短期借用的 `localized` scope 继承语言。Gallery 可切换中英文，并包含国际化演示页。详见 [i18n](i18n.md)。系统语言探测和自动 RTL 布局尚未实现。

## Wabou gallery 对照页

现有 gallery 增加 Data table / Tree navigation：从本地 Wabou 迁移项目数据、筛选/排序/选择与树形展开/选择交互。使用普通 immediate 函数和组件级持久状态；中英文、窄窗口和键盘输入有离屏/语义协议验证。表头与滚动行共享 Grid 列定义。树形页是按钮组合示例，不是完整 ARIA Tree 控件。来源和许可证见 `apps/gallery/reference/README.md`。

## Experimental wgpu quad renderer

`plico-wgpu` independently draws physical-pixel rounded rectangles, vertical gradients,
inside borders, hard rectangular clips and runtime-loaded WGSL materials. The
headless example compares identical quad inputs against Vello at four scales.
It also supports Linux Pango/Cairo text, ordered shape/text draws, bounded glyph
cache recycling and a GPU-only winit window demo. It supports analytical shared paint and an optional `plico-desktop/wgpu` host path.
Default shared-scene damage replay (`PLICO_WGPU_DAMAGE=0` disables it) preserves its color
target and skips unchanged presentation; local GPU pixel-oracle tests and an Xvfb
window exercise pass. A controlled [Linux damage benchmark](experiments/wgpu-damage.md) now records local GPU gains and a scrolling CPU regression; broader application performance and physical-display verification remain pending.
`plico_wgpu::FrameBudget` reports per-frame prepare/upload/encode/submit/wait CPU
time, upload bytes, instance and draw counts, and optional root-encoder GPU
timestamps (`PLICO_GPU_TIMER=1`). The shell HUD and the offscreen `--frames`
capture expose it; definitions and measurement caveats are in
[performance](performance.md).
Shared images share cached source textures across sampling bindings, with nearest/bilinear/Mitchell-bicubic premultiplied-alpha
sampling, independent pad/repeat/reflect axes and ordered batches. Affine image
geometry and image-filled/stroked paths use cached coverage with GPU source sampling; SVG
uses a cached resvg fallback. Static box backgrounds, continuous-corner borders, outlines,
overflow/widget/backdrop clips, scrollbars, text-selection rectangles, image destinations,
axis-aligned SVG destinations and rectangular strokes snap both physical edges independently.
SVGs raster at that exact size; nonzero borders round to at least one physical pixel, and shadow
boxes plus inset-shadow offsets share the same device grid. This matches MyGO's small-icon and
control-chrome policy without snapping glyph outlines, animated transforms or arbitrary curves. Shared native WGSL
effect surfaces use the public shader widget, injected time and bounded local
damage; see [WGSL effect contracts](wgsl-effects.md). It is not yet a
complete Scene backend: solid/linear/radial/sweep gradient paths use a bounded tiny-skia
fallback, including affine linear-brush transforms and multi-stop primitive
fills/strokes; nested clip masks preserve native text drawing. Gaussian box shadows
and Gaussian filtered layers are implemented, with pooled GPU workspaces; general
filter graphs, non-default blends and
Windows/macOS text remain unfinished. Isolated source-over groups use cropped
GPU intermediates. Externally positioned glyph runs use shared memory-font
Cairo/FreeType rasterization, including normalized variable coordinates and
per-glyph/run affine matrices; editor and ANSI terminal fixtures display, with
editor insertion and terminal resize locally verified.
Geometric emboldening, stroke and non-solid glyph brushes remain unsupported. Single-line
editing uses Pango caret/selection geometry. Native paragraphs support
private application fonts and family aliases, and UTF-8
range overrides for size, weight, italic, line height and RGBA color, including
overlapping ranges with later-style precedence. Non-ellipsized line limits preserve
the original shaped prefix and constrain editing geometry; cross-paragraph ellipsis
remains unsupported. Actual input-method candidate-window
sessions and physical-display DPI transitions still need verification. See [experiment and verification](wgpu-backend-experiment.md).

## Renderer maintenance policy

The independent `plico-wgpu` backend is the default desktop renderer and remains
under active development. Skia is the primary rendering-quality reference.
The shared `plico-renderer` crate no longer owns wgpu devices, textures, bind
groups, pipelines or render passes. It retains the backend-neutral scene and
validated shader-effect contract; concrete shader execution lives in
`plico-wgpu`, so Skia and renderer-only builds do not pull in wgpu.
`plico-shell` also no longer republishes renderer modules and scene/widget
types as a parallel facade. Its public surface is limited to window hosting,
extensions and desktop services; backend probes depend on `plico-renderer`
directly.
The Vello backend has been removed. `plico-vello`, the Vello WebGL2 browser
path and the `vello-compare` tooling are deleted; wgpu is the default desktop
renderer and Skia remains the quality comparison target. The browser Gallery
uses wgpu (WebGPU adapter detection with WebGL2 fallback), with Canvas2D
ordinary text and a render-pass Gaussian blur on WebGL2. WebGL2 interaction
tests pass at 1×/2× DPI on Chrome SwiftShader; hardware WebGPU, rich text and
full IME are not yet accepted. See [browser scope](web.md).

The default Cargo features, automatic window/capture selection and new-app CLI
template now use wgpu. Explicit Skia selection remains supported; video examples
that require Skia retain their explicit feature. Changing the default does not
imply full renderer parity or Windows/macOS runtime acceptance.

Shared Scene support in `plico-wgpu` is unconditional; the former `paint-scene`
feature has been removed. `--no-default-features` disables system text but retains
Scene preparation/rendering. `system-text` and `window` remain
independent options. Remove explicit `paint-scene` entries from consumer manifests.

wgpu windows now enable damage by default, following MyGO's default partial-redraw
policy. Damage covering at least half the target falls back to a single full replay;
smaller changes retain multi-rectangle replay and identical frames skip present.
The scrolling benchmark returns to full-render CPU cost while local GPU gains remain.
See [the baseline and post-fallback measurements](experiments/wgpu-damage.md).
