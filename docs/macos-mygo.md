# macOS 与 MyGO 的优化对照

优先参考本地 MyGO `ee3d4bd9bd741f197de231b9fd9b002e9db55577` 的实现，
将通用行为适配到 Plico 的 Rust runtime / desktop host；不是逐字复写 Go 后端。
原生窗口移植的许可证和来源保存在 `crates/plico-shell/third_party/`。

## 已实现

| MyGO 参考 | Plico 行为 | 验证范围 |
| --- | --- | --- |
| `internal/darwin/window.go::roundContent` | AppKit content layer 连续圆角、清空 NSWindow backing、系统阴影、最大化时半径归零 | 原生模块通过 Apple Silicon 目标类型检查与 Clippy；macOS 实机画质由用户验收 |
| `internal/darwin/surface.go::RequestFrame` / `visible` | 隐藏或被遮挡窗口不维持自动逐帧请求；恢复可见后请求完整帧；异步宿主服务继续处理 | 共享调度器覆盖隐藏、到期、未来 deadline、恢复动画；单窗口与多窗口共用策略 |
| 同文件 display link 空闲暂停的调度原则 | `request_rebuild_at` 映射到桌面事件循环 `WaitUntil`；纯计时器不作为持续动画 | 注入时钟验证最早 deadline、稳定绝对时间、组件移除取消、与真实动画并存 |
| `ui/input.go::scroll` 的 `glide := !precise` | 精确触控板位移立即生效，鼠标刻度按配置平滑；精确事件取消已有滚轮动画，基于已呈现位置继续 | 像素小数增量、正反方向、零增量接管、取消旧目标、虚拟列表与嵌套滚动回归 |
| 空闲暂停原则在 runtime 的适配 | tooltip 的 600ms 延迟、滚动条的 800ms 停留使用 deadline；只有实际淡出期间采样帧 | 599/600ms tooltip，799/800/900/1000ms scrollbar，静止无重声明或布局，结束/移除清理 |
| `internal/darwin/{system,app}.go` 和 `ui/preferences.go` | NSWorkspace 初始快照和变化通知，UiContext 可读；系统减少动效不被应用 false 覆盖、玻璃自动不透明回退、语义边框和次要文字增强对比 | 原生模块 Apple Silicon 类型/Clippy 检查；注入偏好测试覆盖恢复、无累积调色和空闲复用；系统设置实机通知仍待用户验收 |
| `internal/surface` FirstFrame / `Window.OnReadyToShow` | macOS + wgpu 启动时保持隐藏，先完成首帧保留纹理和呈现 pipeline，再显示并复制到 drawable；普通帧先编码再获取 drawable | GPU 像素与完整渲染一致、无读回缓冲；实际窗口首次曝光仍由用户在 macOS 验收 |
| `internal/darwin/surface.go` resize / backing properties | 渲染前同步 native surface 尺寸；DPI 事件先同步尺寸再上报 metrics，resize/scale 变化在同次事件中提交新几何；相同尺寸不重复配置 | 共享宿主编译/测试；连续缩放与跨屏实机时序仍待验证 |

精确滚动的 `Pixel` 输入已包含平台提供的细粒度移动和惯性，不再叠加 160ms 插值。
`Line` 输入的 delta 仍由宿主归一化为逻辑像素，`smooth_scroll` 控制其插值。
测试中的平滑滚轮输入明确标记为 `Line`，避免把错误的触控板延迟当作预期行为。

浏览器暂时仍以 rAF 轮询延迟任务，维持既有行为。macOS 的 CADisplayLink 适配见下文；
没有声称完全复刻 MyGO 的呈现时序，也没有 macOS 耗电或 FPS 实测数字。

## 后续检查

- 由用户实测首次显示是否仍有闪烁，以及跨屏 DPI 和 live resize 的呈现时序。
- 继续核对自定义应用颜色是否采用语义 token；自绘控件可读取 `UiContext::display_preferences()`。
- 对照 macOS live resize 的事务呈现，检查连续缩放和跨屏 DPI 变化时的重复配置与资源生命周期。

这些是仍待检查的项目，不是已经实现的能力。

## 原生辅助功能偏好

桌面 `WindowMetrics` 传递 `reduced_motion`、`reduced_transparency` 和 `high_contrast`。
macOS 通过通知唤醒后才重新读取设置；没有周期性计时器。runtime 将系统请求和应用的
减少动效选项按 OR 合并，系统关闭请求时保留应用自己的选项。

`UiContext::display_preferences()` 给自定义组件读取原生请求。`LiquidGlass` 自动使用
不透明 fallback；Modal 在系统减少动效时直接完成挂载和卸载。高对比度按 MyGO 的比例
增强 `Theme.border` 和 `Theme.muted`，保留应用品牌色；每次从原始主题计算，避免反复
声明导致颜色持续漂移。应用硬编码的颜色不会自动变换，推荐使用语义颜色 token。

## 首帧与 drawable 生命周期

wgpu 30 的 Metal 后端会拒绝为隐藏窗口获取 drawable；不能简单地把显示操作移动到
`present` 成功之后，否则窗口永远无法完成首帧。Plico 因此先在已有保留纹理中完成
首帧，提前创建格式匹配的复制 pipeline，在显示前对该次 GPU submission 做一次
最多 1 秒的完成等待，然后显示窗口并获取 drawable、提交 GPU 复制。
这只用于 macOS + wgpu 的首次显示，普通帧不做该等待；没有 CPU 像素读回或额外
窗口大小的截图缓冲。失败会返回错误并使 damage 失效，不把未提交帧记为已呈现。
隐藏首帧在构建或准备阶段遇到不可恢复错误时，宿主返回启动错误并退出，避免窗口
始终不可见、动画循环却不断重试；普通 acquire 的 Outdated / Occluded 等仍走恢复路径。

所有 wgpu 窗口帧都先完成 scene 编码，再持有 drawable。获取失败时丢弃编码，提交
空命令批以刷新 `write_buffer` / `write_texture` 的暂存，并要求下一帧完整重绘。
这避免反复失败时把上传暂存一直留到未来某次成功提交。

首帧这轮没有迁移 MyGO 的 CADisplayLink（后续适配见下文）或动态 `presentsWithTransaction` 切换；
不声称与它的 Core Animation 事务时序完全一致。启动准备完成的像素测试运行于
Linux / Vulkan，不能证明 macOS 窗口服务器的首次曝光已经无闪烁。

## Toast 停留期间休眠

对照 MyGO `ui/toast.go` 的 `c.After(next)`，Toast 入场动画结束后使用
`request_rebuild_at` 等待最早的到期时间，不再为了几秒钟的停留时间持续请求帧。
悬停暂停时取消到期唤醒，离开后按剩余时间重新安排；入场和退场仍逐帧驱动。
`toast_motion` 的虚拟时钟回归检查停留期间无动画请求、无 adapter 同步访问、
到期正常关闭，并检查悬停取消 deadline。此调度优化适用于所有宿主，
macOS 桌面复用已有 WaitUntil 调度，无新增平台计时器。

## 私有 CoreText 应用字体

对照 MyGO `internal/text/coretext_darwin.go` 的 `register`，使用
`CTFontManagerCreateFontDescriptorsFromData` 保留私有描述符，按原生 family 和应用别名
查找最近的字重/斜体 face。没有进程级字体注册，不修改系统字体环境。
`macos_text::FontCollection` 独立拥有描述符与最多 256 个尺寸/样式缓存；
`Request::fonts` 可选择私有集合。已创建的 Line / Paragraph 自己保留原生字体，
不依赖集合的后续缓存存活。每个集合最多接受 64 次注册和 128 MiB 字体数据。

wgpu CoreText 桥接不再因应用注册字体而拒绝所有后续布局。有效注册清除派生布局和
栅格缓存；非法数据返回错误或警告，保留现有字体。引擎重装（如 DPI 更新）按共享
FontAsset 存储身份去重，不反复积累描述符。普通系统字体仍可使用。

原生字体与渲染桥接回归包括别名、上下文隔离、缓存替换、非法数据、集合销毁后的
存活 Line 和引擎重装。Linux 环境只能对 Apple Silicon 目标编译这些测试，
不能声称已运行 CoreText 像素断言；macOS 运行验证仍由用户执行。

## 空闲窗口的 drawable 与渲染缓存

对照 MyGO `internal/gpu/metal/metal.go` 的 `armTrim` / `trimIfIdle` / `trim`：
macOS + wgpu 窗口两秒没有成功提交新画面后，宿主执行一次资源回收。派生路径、图片、
隔离层、效果和文字栅格缓存被释放，保留目标及截图缓冲也被释放；复制 pipeline 的
旧 source binding 一并清除，避免它继续持有已释放目标的纹理视图。

wgpu surface 暂时配置为 1×1（Metal 后端对应 CAMetalLayer.drawableSize），
逻辑窗口尺寸保持原值。Core Animation 保留已呈现的画面；下一次 present 恢复原尺寸，
由于保留目标及 damage 已失效，即使 scene 未变也必须完整重绘。shader/pipeline 和
应用字体注册保持热态。遮挡/内存警告也走回收路径。

维护定时使用已有事件循环的 WaitUntil，与 UI deadline 取较早值，不驱动应用重绘，
也不增加周期轮询。只在实际提交新帧后重新安排；跳过的 unchanged present 不推迟回收。
单窗口与多窗口共享相同调度。后续 Linux 的回收适配见 `linux-mygo.md`；
Linux 保持真实 surface extent，不缩小 swapchain 配置。

GPU 回归验证目标字节清零、静态 scene 必须重建以及重建像素一致；宿主测试验证维护
唤醒不吞掉动画或更早的 UI 定时。Linux GPU 测试不能验证 Core Animation 显示保持，
macOS 上静止两秒后的画面与恢复交互、实际 RSS 变化仍需实测，未宣称具体内存降幅。

## NSView display link 动画调度

对照 MyGO `internal/darwin/surface.go` 的 `RequestFrame` / `mygoTick:`：
macOS 14+ 创建与 winit NSView 关联的 CADisplayLink，加入主 run loop 的 common modes，
采用默认帧率范围以跟随所在屏幕。连续动画请求通过 tick 唤醒宿主，由原有 wgpu
路径提交，组件插值仍由 Plico 的 Clock / FrameTime 驱动。连续请求不反复暂停/启动
原生 link；空闲、隐藏、遮挡或等待 UI deadline 时暂停，关闭窗口前 invalidate。

tick target 仅持有调度握手和事件循环 wake，不保存应用/UI 状态引用。一次请求只
产生一个待处理 tick；宿主停顿不会积累重绘队列。输入、首帧、缩放等已请求的直接
重绘继续立即执行，并消费可能未处理的 tick；不把首个隐藏窗口交给 display link
等待曝光。没有 NSView display-link selector 的旧系统自动使用原调度。

空闲资源回收和 UI deadline 仍用 WaitUntil，不靠 display link 定期唤醒。单窗口与
多窗口共用帧调度入口。此适配未迁移动态 `presentsWithTransaction`，也不声称已
验证 ProMotion、AppKit tracking/live resize 的实机帧时序。握手和宿主测试可在 Linux
运行；原生模块使用 Apple Silicon 目标做类型/Clippy 检查，macOS 动画效果待实测。

## AppKit 系统字体选择

对照 MyGO `internal/text/coretext_darwin.go::systemFont`，系统 UI 字体通过
`NSFont.systemFontOfSize:weight:` 按原生字重选择，而不是先生成常规 CTFont 再组合
traits 字典。`monospace` / `ui-monospace` 使用
`NSFont.monospacedSystemFontOfSize:weight:`；macOS 10.15 之前回退 Menlo。
显式指定 `Menlo` 仍选择该命名字体，不改变其他命名字体或私有应用字体选择。

NSFont 经 objc2-app-kit 的公开 toll-free bridge 转为自持有 CTFont，局部 autorelease
pool 在字体缓存 miss 后立即排空；缓存命中不重复查询 AppKit。斜体选择只修改 italic
trait，保留 NSFont 选出的字重。尺寸仍按精确逻辑值缓存，不对光学尺寸取整。

新增原生回归对照 AppKit 在多个字号/字重下的 PostScript 名称，检查 pool 排空后的
光标和栅格有效性、等宽字形 advance 及 generic family 的桥接。它们可编译到 Apple
Silicon，实际执行需要 macOS。没有从代码审查推断进程 RSS 降幅。


尺寸事件处理现由单窗口和多窗口共用：读取 native 最新 extent，真实变化才在事件
内绘制，过时/重复事件不再额外配置和重绘；扩展只在 metrics 实际变化时收到通知。
Linux Xvfb 原生窗口回归验证共享处理流程，不能替代 macOS live resize 实机测试。
wgpu Metal 已支持 `presentsWithTransaction` 所需的 waitUntilScheduled / present 顺序，
但 MyGO 还依赖 CA 提交轮次与重复绘制保护；尚未仅通过打开开关迁移该行为。


## 系统多击间隔

MyGO macOS 从 NSEvent 取得 clickCount，其他后端通过 `platform/clicks.go` 结合系统
阈值判断。winit 没有直接暴露原生 clickCount，本轮通过 `NSEvent.doubleClickInterval`
把系统双击间隔下传为 `FrameSource::set_multi_click_settings`。初始窗口和指针按下时
读取，非逐帧查询；偏好未变时不重复更新 source。

`MultiClickSettings` 保持宿主无关，Source 的可选择只读文字按注入的 Clock 判断，
变化时只重置点击序列，保留可见选择，不主动重建 UI。默认 500ms / 4 逻辑像素保留
既有运行时行为。ZoomSource 反向缩放空间阈值，保持窗口坐标下的点击容差一致。

SurfaceSource 将时间阈值转给托管 Widget；终端采用该间隔，保留自身网格的空间规则
和脱离宿主时的既有 400ms fallback。框架其他 Widget 可以覆盖
`Widget::set_multi_click_interval`；默认忽略，不宣称所有编辑器已接入。

虚拟时钟回归覆盖长/短间隔、偏好改变后保留选择、缩放以及终端单词选择。
macOS 原生读取模块经 Apple Silicon 类型/Clippy 检查，系统面板实测仍由用户完成。
文字选择路径没有取得 NSEvent 原始时间戳或 clickCount，忙主线程下的事件排队语义
仍不同于 MyGO；空间阈值也尚未从系统读取。不把阈值下传声称为完整的原生点击计数。


## 自定义标题栏双击

对照 MyGO `internal/darwin/window.go::TitleBarDoubleClicked`，统一拖窗入口在 macOS
上检查当前原生 NSEvent：只处理同一窗口的左键第二次按下。与 winit 的拖窗实现一样，
调用必须处于当前指针手势中。单击、其他窗口和非左键事件保留既有拖窗路径。

双击读取 `AppleActionOnDoubleClick`：`None` 消费事件但不操作，`Minimize` 调用
`performMiniaturize`，其余值按 MyGO 的 fallback 调用 `performZoom`。不把应用自己的
最大化设置当作系统偏好。WindowCommand::StartDragging 和扩展的 start_window_drag
共享入口，Plicowarden 的现有拖动区域无需单独实现。

每个窗口仅记住最后处理的 eventNumber / timestamp 标识，不保留 NSEvent；相同事件
产生的重复命令不会再次切换缩放状态。NSUserDefaults 只在合格双击时读取，原生临时
对象在局部 autorelease pool 中释放。该路径使用原生点击计数，但不改变文字选择
仍基于注入时钟判断的边界。

宿主回归覆盖动作映射、普通拖动和跨窗口过滤、重复事件处理。AppKit 桥接使用
Apple Silicon 目标做类型/Clippy 检查，未在 Linux 上声称验证真实 macOS 窗口动作。
系统“缩放 / 最小化 / 不操作”的表现及无边框窗口的 AppKit 行为仍需用户实测。
