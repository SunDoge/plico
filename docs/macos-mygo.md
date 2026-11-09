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

## 当前验收范围与未完成项

运行原生验证入口：

```sh
./scripts/check-macos-native.sh
```

脚本只在 macOS 执行，默认写入忽略提交的 `reports/macos-native/`；可通过
`PLICO_MACOS_CHECK_REPORT_DIR` 指定目录。记录 Git revision、系统/架构和工具链，
分阶段运行 CoreText、竖排、bridge、原生 GPU、稳定位图采样及宿主测试。阶段失败
立即返回非零，过滤器没有执行任何测试也视为失败；不把“0 tests”当成绿色验收。
它不启用 Skia，也不运行 Windows 后端。需要可用 Metal adapter 以及完整 macOS
开发工具链。当前 Linux 仅验证了平台拒绝路径、脚本语法和实际 Apple 目标源码编译，
没有运行脚本内的 macOS 原生测试。

新增竖排回归交替生成不同颜色的横排/竖排文字，核对 1/1.25/2 倍下可见像素与重复
竖排图像一致性，覆盖共享不可变色彩空间不应混用 CGContext 可变颜色状态的契约。

| 项目 | 已有实现/证据 | 仍需完成的证据或工作 |
| --- | --- | --- |
| CoreText 字体、私有字体、词/行选择、栅格复用 | 实际 Apple 目标实现及测试可编译；入口执行原生像素、几何和复用回归 | 在 macOS 执行入口并检查真实输入/字体显示 |
| 窗口轮廓、初帧露出、空闲回收、隐藏/恢复、resize | 已实现宿主路径；共享逻辑有 X11/wgpu 回归；脚本含主线程不透明窗口生命周期程序 | macOS 实际执行；自定义透明圆角与 CA 视觉一致性仍需单独验收 |
| NSView display link / ProMotion | macOS 14+ 使用原生 display link；旧版回退现有宿主调度 | 60/120 Hz、跨显示器、遮挡恢复的实机节奏和功耗数据 |
| 原生 clickCount / 原始事件时间 | 标题栏特殊路径使用当前 NSEvent；普通文字使用系统间隔与注入时钟 | 需要在原始事件与 winit 派发边界保留身份/计数/时间；不能在事后盲读 currentEvent |
| CA transaction 同步呈现 | wgpu Metal 有相应底层能力，Plico 未打开该层属性 | MyGO 的 CA 提交轮次保护、重复绘制和 resize/同轮事务组合必须一起设计与验证 |
| 异步 drawable 获取 | 默认 macOS 宿主启用单槽后台获取；X11 真 surface 延迟/清理回归及 Apple 目标编译通过 | Metal 实机指标；pending GPU 编码重叠；与 CA transaction 策略联合验证 |
| 内存和交互性能 | 有界缓存、资源释放及像素正确性回归 | macOS RSS/footprint、滚动 p95/p99、连续 resize、原生动画的实测对照 |

### 为什么不从 currentEvent 补普通点击计数

本地 `winit-appkit 0.31.0-beta.2` 的 `view.rs::queue_event` 将回调交给
`app_state.rs::maybe_queue_with_handler`。没有正在处理回调时可以立即派发；嵌套处理
期间则放到 run-loop closure 队列，稍后才调用应用。PointerButton 只携带按钮、位置等
通用字段，没有原 NSEvent 的计数、事件编号和时间戳。此时即使 currentEvent 的窗口、
类型和坐标碰巧相同，也不能证明它是队列中这条输入的原事件。只补一个即时 NSEvent
读取会留下错误关联，不能作为与 MyGO `surface.go::mouse` 直接复制原 clickCount
等价的实现。该差异保持显式待办，不用更宽松的匹配掩盖。

脚本全绿仅证明表内自动回归覆盖的部分，不能将上述待办及实机指标标记为已完成。

## 单槽后台 surface 获取

参考 MyGO `internal/gpu/metal/present.go` 的 presenter，默认 macOS 宿主通过
`WgpuWindow::set_surface_wake` 安装事件循环代理后启用后台获取。每次成功呈现后
最多预取一个 `CurrentSurfaceTexture`；同一个 worker 在空闲时等待条件变量，没有
获取队列或周期轮询。独立使用 WgpuWindow 而未安装代理时仍使用原有同步路径；
线程创建失败记录诊断并保留同步路径。Linux 默认路径本轮没有开启后台获取。

- 普通同尺寸帧遇到尚未完成的获取时返回 pending，UI 线程继续处理输入。结果到达
  只在确有帧等待时设置一次 signal 并唤醒 event-loop proxy；宿主将它转换为 redraw。
  静态窗口的预取自行完成不会制造额外 redraw。pending 不走失败呈现的立即重试。
- 首帧 ready-to-show 和真实 surface 几何变化使用同步路径。fit/configure、trim、
  surface 丢失重建及窗口析构前先 drain，等待 in-flight fetch 并释放旧 texture。
  这些生命周期操作仍可能等待获取结束，与 MyGO 的同步保护相同；不声称所有
  resize/关闭都完全没有主线程等待。析构在 renderer/surface/window 字段释放前 join。
- worker 唤醒只调用代理，不能从后台调用 winit Window::request_redraw：当前 AppKit
  实现的 maybe_wait_on_main 可能等待 UI，与 UI 等待 worker 形成环。
- wgpu 30 的 Surface/SurfaceTexture 通过 Send/Sync 类型约束跨线程持有；真正的
  acquire 在 worker，配置及编码/提交仍归宿主。配置期间绝不保留旧 texture。
- 当前 bridge 在 ready 后编码最新 scene，没有跨获取间隔保留已编码 command buffer。
  它与 MyGO pendingFrame 的 GPU 编码重叠不同。后续已将 preparation/planning
  分开，pending 不推进 damage history（见下文）；GPU 编码重叠仍需结合 Renderer
  共享可变 buffer 和显式 capture 生命周期继续优化。
- 等待实际帧时暂停 idle 维护期限，防止维护清理撤销唯一的完成唤醒；完成后重新安排
  两秒一次的回收。显式遮挡/关闭仍可 drain 清理，并在恢复显示时完整重绘。

三项并发回归使用受控通道验证单槽、等待者唤醒、静态预取不唤醒、drain/drop 释放
实际 owner 以及 fetch panic 后不永久等待。真实 X11/wgpu 回归将受控延迟注入实际
surface.get_current_texture，核对 UI 先返回 pending、完成 signal 只消费一次、随后帧
不会误判 unchanged、截图像素正确，以及 drain 后真实 configure 不会因旧 texture
仍存活而 panic。这验证共享资源协调，不冒充 Metal 驱动时序测量。

```sh
cargo test --locked -p plico-wgpu --features window --lib surface_acquire::
xvfb-run -a cargo test --locked -p plico-wgpu --features window --lib native_surface_resize_waits -- --ignored
```

原生验证脚本已加入 worker 并发阶段。Apple 目标实际实现/测试、Linux Clippy 和 WASM
检查通过，宿主现有回归保持通过。尚未在 Metal 真窗口运行上述默认路径，也没有
macOS 主线程等待、帧时间 p95/p99 或 RSS 对照；CA transaction 联合策略仍未开启。

后台获取默认开启；可在进程启动前设置 `PLICO_WGPU_ASYNC_SURFACE=0`（或 `false`）
进行同一二进制的同步/后台 A/B 对照。开关只在安装宿主代理时读取，不在帧循环查询。
原生验证报告会记录该设置。worker panic 会报告错误并停用该 worker，保留宿主已有
的有限同步恢复重试；这不保证能恢复已损坏的驱动/设备。

## drawable 等待不再迫使局部 damage 失效

进一步对照 MyGO 保留待绘帧变化信息的原则，把 Plico 的 frame preparation 与
破坏性 damage planning 分成显式的两个阶段。PreparedFrame 保存校验后的不可变
scene、extent 和 background；丢弃它不会推进上一帧的 damage/background。读取
worker readiness 只探测槽位、安排唤醒，不消费 texture。就绪后才进入 damage 计算，
然后按原有计划编码/提交。实际编码、获取、提交失败仍走既有的保守失效处理。

因此普通等待无需复制整份 tracker 或清空其历史，最新状态可以继续相对真正已经画出
的内容计算局部更新。新 scene 仍在等待前完成校验，unsupported 命令不会因为 readiness
门控而被当成一个成功提交。未变化的帧满足请求后会消费残留通知，保留预取 texture，
避免迟到的 proxy wake 额外生成同一帧。

真实 surface 的受控等待回归改为小矩形更新：就绪后 coverage 大于零且小于画面 10%，
并与全量重绘的每个像素一致。另一个无需原生窗口的 GPU 回归丢弃包含不同背景的
PreparedFrame，核对原画面仍能 unchanged skip，后续局部更新与全量 replay 一致。
该回归已加入 macOS 原生验证入口。Linux GPU、共享 surface 回归和 Apple 编译检查
通过，Metal 真窗口帧时间仍未在当前环境测量。

```sh
cargo test --locked -p plico-wgpu --features window --lib abandoned_prepared_frames_preserve -- --ignored
```

这一步保留 CPU damage 基线，没有迁移 MyGO 的 pending GPU command-buffer 编码
重叠。CA transaction 配合与实际 Metal 吞吐/延迟验证仍是独立未完成项。

## 主线程真窗口生命周期验证

`surface-lifecycle` 在进程主线程创建一个真实、不透明的 winit/wgpu 窗口：初帧 hidden
ready-to-show，60 次连续变化绘制，连续请求三个尺寸后采用 native 最新 extent，
两秒空闲回收后恢复绘制，隐藏/清理/再次显示，最后校验恢复图像并自动退出。
事件循环使用截止时间和 worker proxy，不用轮询等待。检查未完成而关闭窗口、丢失
完成状态、缺少预期捕获像素等都会返回非零；进度超过 20 秒也会报错。该截止时间
不能中断自身已经卡住的原生调用，不能冒充外部进程强制超时。

```sh
cargo run --locked -p plico-wgpu --features window --example surface-lifecycle
PLICO_WGPU_ASYNC_SURFACE=0 cargo run --locked -p plico-wgpu --features window --example surface-lifecycle
```

开关不改变编译产物，可对同一二进制对照。输出包含接受帧数、等待次数、warmup 总
时间、成功 present 调用的 CPU p95 和空闲 readiness 通知数量。pending 调用耗时没有
混入“成功调用 p95”，等待体现在 warmup 总时间和 pending 次数中；这些不是整个
应用的输入延迟、GPU 时间或真实显示刷新帧率。保持测试窗口可见，避免人为遮挡把
正常 occluded 状态混入测量。窗口只显示检查图形，不是 Plicowarden 视觉验收。

该程序已在 Xvfb/X11 的真实 surface 运行通过，完成 64 帧与生命周期阶段；Linux 默认
路径为同步，pending=0 不能用于证明 macOS 后台分支的性能。实际 Apple 目标程序
通过 Clippy 编译检查，但当前环境没有运行 Metal 窗口。macOS 验证脚本现在会执行
此程序，并要求正常退出及明确的完成标记，将指标写进 summary；失败或缺失标记
不能算通过。自定义透明圆角、标题栏交互、ProMotion、多显示器和进程 RSS 仍不在
这个程序的验证范围。

### 隐藏请求前的获取屏障

框架的 mutable-visibility Hide 和 WindowCommand::Minimize 入口现在先调用
prepare_hide：停止动画 clock，趁 native view 仍可见时 drain 获取任务并释放预取
texture，再执行隐藏/最小化。这个阶段保留 target/readback 像素，供系统窗口动画
使用；遮挡通知或后续空闲维护仍按已有策略回收派生资源。后端检查程序使用相同入口。

这是对 MyGO presenter 在资源生命周期切换前 wait/drain 原则的适配：wgpu Metal 的
后台获取不能用主线程 NSWindow occlusion 查询提前跳过已隐藏窗口，因此先隐藏再
等待可能延长获取等待。没有将 MyGO Window::Hide 本身描述成拥有此屏障；这是 Plico
在 wgpu 边界补的顺序。系统直接发起的隐藏、系统原生交通灯或菜单最小化等仍可能先到
occluded 事件，不能保证所有操作都没有 in-flight 等待。

真实 surface 回归验证 drain 后仍保留 target/readback，且随后 configure 不会因
旧 SurfaceTexture 存活而 panic。X11 主线程检查程序和宿主测试通过，Apple 实现/程序
编译检查通过；未把 X11 耗时当作 Metal 最小化动画或隐藏延迟数据。程序捕获的图像
来自 Renderer 保留目标，不读取 WindowServer 最终合成结果，也不能代替视觉验收。

检查程序现在读取 `read_retained_rgba`，不再用会重新绘制 scene 的截图接口验证旧帧。
它核对恢复后目标的真实尺寸、背景像素和最新移动矩形的边缘像素；受控 surface 局部
重绘回归也直接读原目标，再与完整 replay 对照。该显式 readback 会等待 GPU 并可能
创建读回 buffer，不属于正常呈现路径；没有保留目标时返回 None，不为了检查重新
分配 target，也不推进/失效 damage history。现有“按指定 scene 生成截图”行为保留。

输出还记录实际安装的 worker 模式（async/sync），不是只记录请求的环境变量。
macOS 若请求后台模式而 worker 无法安装，检查程序报错；同步 A/B 模式仍可显式
选择。这里的目标像素仍不是 WindowServer 最终合成输出，系统视觉验收范围保持独立。

### 自定义标题栏双击最小化也经过屏障

NativeInput 现在在当前拖窗手势中先解析系统双击动作，返回持有 NSWindow 引用的
PreparedTitlebarAction；Shell 在 Minimize 动作前调用相同 prepare_hide，再执行
AppKit 的 performMiniaturize。Zoom / Ignore 保持原动作，不额外等待获取任务。
同一 NSEvent 的重复命令产生 Ignore，不重复执行动作或屏障。局部 autorelease pool
排空后，待执行动作仍有自己持有的 native target，不借用池内临时对象。

该顺序补齐的是框架自定义拖窗区域路径。系统原生交通灯/菜单直接发起的动作不经过
这个解析器，仍依赖后续 occluded 通知处理。系统设置解析和事件去重的现有 portable
回归、宿主 68 项测试与实际 AppKit 模块的 Apple 目标 Clippy 检查通过；当前 Linux
没有执行双击系统偏好对应的 macOS 交互。实机需在系统选择“双击标题栏最小化”后，
对有持续绘制的自定义标题栏确认动作及恢复行为，不能用普通不透明检查窗口代替。
