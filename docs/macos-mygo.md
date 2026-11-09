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

输入框现也使用共享多击配置，支持拖选、双击选词和三击选视觉行。
CoreText 从已排版行读取范围；共享行为和验证边界见 `linux-mygo.md` 的
“编辑框选择复用原生布局”。macOS 事件和实际窗口手势仍待实机验证。

## 字体缓存避免整批失效

对照 MyGO `internal/text/coretext_darwin.go` 保留 primary/style CTFont、复用原生
shaping 缓存的生命周期，Plico 的精确字号缓存达到 256 个 face 时改为淘汰最久
未使用的一个 face。此前会清空整批缓存；连续字号动画采样填满缓存时，普通标签
所用的字体也会一起丢失。新的策略让持续被使用的常用字号保持热态。

上限仍为 256，字体大小不取整；缓存命中通过借用 family 名称查找，不新增字符串
分配。只有未命中且满额时扫描有界缓存，移除空 family 键，避免 family 名称独自
累积。已排版的 Line 持有自己的 CTFont 引用，淘汰不破坏其文字几何或后续绘制。
应用字体注册依然主动清空相关解析结果，避免旧的 fallback 继续遮蔽新字体。

原生测试覆盖三个缓存容量的动画采样混合常用字号、容量与空 family 回收、引用
存活，以及使用序号溢出后的顺序。Apple Silicon 目标的实际 CoreText 实现及测试
已通过 Clippy 编译检查；当前 Linux 环境不能执行这些 macOS 原生测试，亦未测得
macOS 帧时间或 RSS 的变化，不据此宣称实机性能提升幅度。

## 滚动时复用文字位图

对照 MyGO `internal/text/coretext_darwin.go` 的 glyph 栅格缓存及 GPU 绘制期裁剪，
Plico 的 CoreText 行位图不再直接把每帧精确 viewport 交集作为缓存键：

- 小于等于 64 KiB 的行复用完整 ink 位图，精确 viewport 仍在 GPU 绘制时裁剪。
- 超长/大字号行仅栅格化可见区域附近，以行空间的 128×32 物理像素网格稳定窗口；
  预留 2 像素采样边界，避免双线性采样在临时裁剪边缘夹取错误像素。不拆分为
  多个拼接位图，不新增文字接缝。
- 完全不可见的行在分配前跳过。扩展后会超出既有 16384 边长或 64 MiB 上限时
  回退精确窗口，保留大内容的既有拒绝边界；总 raster LRU 仍有计数和字节上限。
  max-lines 的固有硬边界继续限制位图，不能被 viewport 缓存扩展越过。

同一字体、颜色、密度和 phase 下，滚动边缘的文字可以复用同一个 ImageData ID，
因此 CPU 栅格缓存命中也能继续命中 GPU 上传缓存。没有量化布局、字号或文字位置。
本轮没有把 MyGO 的逐 glyph atlas 替换进来；适配的是栅格内容与 viewport 裁剪分离原则。

验证：5 项可在 Linux 执行的窗口规划回归覆盖负坐标、可见像素包含、有限过绘制、
极长 ink 以及预算回退。120 次单像素滚动中，精确窗口产生 120 个不同键，稳定窗口
不超过 10 个。实际 wgpu 像素对照覆盖 0.75/1/1.25 缩放及四个亚像素位置，与完整
位图应用相同精确裁剪的结果逐通道误差不超过 1，可重建命令：

```sh
cargo test --locked -p plico-wgpu --lib native_raster_window
cargo test --locked -p plico-wgpu --lib stable_native_raster_windows_preserve -- --ignored
```

CoreText 原生 crop 像素对照、移动 viewport 后 ImageData ID 复用，以及 macOS GPU
滚动时不新增 raster/upload 的回归已通过 Apple Silicon Clippy 编译检查；当前环境
没有执行这些原生测试。后者可在 macOS 运行：

```sh
cargo test --locked -p plico-wgpu --lib scrolling_native_line_reuses -- --ignored
```

这些结果不等同于 macOS 滚动帧时间或 RSS 的实测降幅。

## 不可见窗口不因后台 redraw 恢复呈现资源

对照 MyGO `internal/darwin/surface.go::RequestFrame` 的可见性判断：AppKit 可以要求
已经遮挡的 view 重绘，后台消息也可以请求 hidden window redraw。此前 Plico 只在
动画调度时检查可见性，直接到来的 RedrawRequested 仍会 build 并尝试呈现，可能把
刚经遮挡/空闲回收的 GPU 资源重新建起来。

App 共享 redraw 路径现于显式截图处理之后、窗口呈现之前检查 WindowPresence 和
occluded。不可见时暂停原生动画 clock、清除呈现重试与未变化帧退避，不获取 drawable，
不提交普通窗口帧。初次隐藏启动的 ready-to-show 帧保留；显式 Hide 生命周期不会
被启动例外覆盖。恢复显示/解除遮挡使用现有 invalidation + request_redraw 完整呈现。
单窗口与多窗口共用这一逻辑。

继续执行 source 声明、host actions、语义同步和显式截图，保持后台任务与自动化契约；
本轮没有宣称不可见时完全不执行 UI CPU 工作。显式截图可能临时重建 offscreen 和
readback target，不可见时在截图完成（含失败）后再次回收，避免没有后续普通呈现
来安排维护期限而永久保留这些资源。

共享宿主原生回归在 Xvfb 下实际创建 winit/wgpu 窗口：模拟 Occluded 事件，核对
异步轮询、声明继续处理、截图 PNG 尺寸/完成回调、无呈现维护重启或重试，以及
恢复显示后重新呈现。另保持 native window 映射而只切换生命周期，确保隐藏分支
不是因为驱动忽略 hidden drawable 才通过。测试命令沿用：

```sh
xvfb-run -a cargo test --locked -p plico-shell --lib native_resize_uses_latest_extent -- --ignored
```

这证明共享 App 路径，不替代 macOS 实际遮挡事件、Core Animation 或 RSS 实测。

## 仅为真正的帧恢复 drawable 尺寸

对照 MyGO `internal/gpu/metal/metal.go::render` / `fit`，WgpuWindow::resize 现在只记录
期望的非零尺寸，不立即执行 surface.configure。真正呈现时先取得窗口最新实际 extent，
完成场景规划、确认不是 unchanged skip，再适配 surface。因此隐藏期间的 resize/DPI
通知不会仅因更新尺寸就把空闲时缩到 1×1 的 drawables 恢复，多次通知也不会逐次配置
过时尺寸。记录实际配置尺寸与期望尺寸分离；macOS idle trim 同步记录实际 1×1。
初次 ready-to-show 仍在露出窗口之前完成所需 surface 适配。

资源准备及显式离屏截图也会安排一次两秒空闲维护：配置或编码之后即使获取 drawable、
提交失败，也不会因为缺少“成功 present”而永久失去回收期限。成功呈现继续刷新期限，
unchanged skip 不刷新，没有新增周期轮询。离屏截图使用自己的目标，不恢复窗口
drawable；App 不可见时截图后即时回收的行为保留。

真实 X11/wgpu 回归用实际 surface.get_configuration 观察：先配置为 1×1，连续 resize
及零尺寸通知保持 1×1；离屏截图不改变它，且一次维护可释放实际 target/readback。
下一次 present 恢复到 native 当前尺寸而非排队的旧尺寸。期望尺寸往返但实际窗口未变
时，后续帧仍可 unchanged skip。另复跑共享宿主 resize/遮挡/截图/恢复回归：

```sh
xvfb-run -a cargo test --locked -p plico-wgpu --features window --lib native_surface_resize_waits -- --ignored
xvfb-run -a cargo test --locked -p plico-shell --lib native_resize_uses_latest_extent -- --ignored
```

Linux 原生回归和 Apple Silicon wgpu 实现/测试的 Clippy 编译检查通过；尚未在 macOS
执行实际 resize、ready-to-show 和空闲恢复测试，不据此声明实机内存降低幅度。

## 横排/竖排共享线程内 sRGB 资源

对照 MyGO `internal/text/coretext_darwin.go` 在引擎中持有 sRGB color space，
CoreText 横排和竖排栅格化现在通过同一个线程内 OnceCell 复用标准 sRGB 对象。
此前每次创建 CGContext 都会再次调用 CGColorSpaceCreateWithName。新的 helper
借用缓存对象，由 CGContext 持有自己的原生引用，避免每次调用额外 retain/release
一个本地 color space owner。每个使用文字渲染的线程最多保留一个对象，随线程退出
释放；不增加跨线程 CF 对象传递或全局析构顺序。初始化失败仍返回 Allocation，
不写入失败状态，后续调用可重试。

CGContext 本身仍为每个位图单独创建，使用相同的标准 sRGB、premultiplied RGBA8、
采样和 AA 参数。没有共享可变 context 或像素存储。本轮仅减少重复的原生资源查询，
不宣称已经测出可见帧时间改善。实际横排、竖排实现及已有原生测试通过 Apple Silicon
Clippy 编译检查；macOS 像素/性能运行验证仍未在当前 Linux 环境执行。

## 字体 family 解析不再跟随文字和宽度重复探测

对照 MyGO `internal/text/coretext_darwin.go` 对 primary/style 字体和属性的复用，
CoreText bridge 将 family 解析与具体段落排版分开。此前每次段落缓存未命中都会先
构建空字符串的 CTLine 探测字体，即使请求的是系统字体，随后才真正排版用户文字。

系统 UI/系统等宽字体和已注册私有 family 直接使用已知解析路径；其他 family 仍保留
原有 native family/PostScript 名称核对与按序 fallback。解析成功的完整 family 列表
放入最多 128 条的 LRU，编辑文本、改变宽度或字号时可复用解析结果。入口仍限制
family 列表为 4096 字节；名称载荷的保守上界约 1 MiB，不保存额外的文字内容。
MacosTextCacheStats 的既有 byte 字段仅统计 layout/raster/intrinsic，不包含这些名称。

注册新字体、清理 provider/空闲缓存时同步清空 family 解析结果，确保原先选中系统
fallback 的列表能在应用字体注册后改选新字体。纯失败的列表不做负缓存。

原生回归覆盖系统字体零额外探测、缺失字体后 fallback、文本/宽度变化不重复探测、
128 条上限及注册私有字体后撤销旧 fallback。实际 CoreText 实现与这些测试通过
Apple Silicon Clippy 编译检查；当前 Linux 环境未执行原生测试，没有据此给出
macOS 实测排版耗时降幅。
