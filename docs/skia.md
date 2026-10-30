# Skia / Vello 绘制对照实验

状态：`plico-skia` 已提供 CPU raster 和 Ganesh Vulkan 离屏 GPU consumer；
另提供 Vulkan 桌面窗口后端，桌面默认优先使用 Skia；Vello 可单独启用。

## 已实现

普通 Skia 窗口现在使用 SkParagraph，详见下节。原有 `compare` 探针继续将
同一份 Parley glyph Scene 分别交给 Skia CPU 和 Vello GPU，以隔离排版差异。
这个对照模式使用相同字体字节、TTC index、glyph ID、位置和字号；Skia 直接
绘制这些 glyph。字体文件按 Blob ID 缓存数据，TTC face 共享该数据；
小字号使用灰度抗锯齿、subpixel positioning 和按命令启用的 normal hinting。

支持 solid fill 路径、几何 transform、nonzero/even-odd fill、变换后的路径 clip，
对称端点描边、普通透明合成图层、高斯模糊圆角矩形 box shadow（与 Vello 的
BlurredRoundedRectangle 语义一致，sigma 即 std_dev，退化与反向矩形取绝对值），
以及 solid fill glyph run 的合成加粗与逐字形倾斜变换。SVG 直接由 resvg
消费保留的 usvg tree，按显示缩放光栅化并缓存，覆盖 filter、mask 和 pattern。
每次 render 清空旧画面并重置 clip stack，失败不返回部分像素，也不能把失败帧
导出为 PNG；下一帧可以正常重试。像素 API 返回 tightly packed premultiplied
RGBA，PNG 由 Skia 编码。

`plico-skia` 自身只在启用 `raster` 或 `vulkan` 时加载 skia-safe；桌面宿主默认启用 Skia。
项目固定使用 rust-skia 0.153.3，并启用 `no-compile`：只允许使用匹配 target 和 feature 的
预编译库，下载失败或缺包时立即报错，绝不自动从源码构建。首次构建需要能访问预编译包，
之后可复用本机缓存；`SKIA_BINARIES_URL` 可配置可信镜像。

Windows 开发和 CI 使用 `x86_64-pc-windows-msvc`；该版本提供匹配 Vulkan + textlayout 的预编译包。
上游当前没有对应的 `x86_64-pc-windows-gnu` 预编译包，因此该组合会明确失败；使用 MSVC
工具链，或者仅启用 Vello。Linux x86_64 和 Windows MSVC 的目标组合已验证预编译包可用。
其他平台与 feature 组合必须确认上游提供匹配包，不能将这些验证扩展到所有平台。
当前 consumer 复用 renderer 的指令流，以及直接依赖的 kurbo / peniko 共享
类型与依赖，不宣称已完全解耦 Vello。

## SkParagraph 普通文字路径

选择 Skia 窗口 renderer 时，宿主自动安装 SkParagraph 引擎；启动日志显示
`text_backend="skparagraph"`。Taffy 的 intrinsic width、高度、基线，普通文字
绘制、按钮文字对齐、只读文字命中/选区均使用该引擎。Vello 默认路径继续使用
Parley。这里共享声明与交互契约，不要求两个后端产生相同字形或断行。

`plico_renderer::paragraph` 提供后端中立的请求、metrics 和选区接口；
Scene 保留 UTF-8 段落声明及 transform，既不保存 Skia 指针，也不跨线程传递
SkParagraph 对象。Skia 宿主的测量与绘制共享一个有界段落缓存（512 项，
命中时再次核对完整请求）；FontCollection 统一管理系统 fallback 与注册字体。
应用字体别名、字号/字重/斜体、字间距、行高、富文本范围、对齐、换行、
单行省略及多行截断均映射到 SkParagraph。命中和选区在接口处转换 UTF-8 byte
与 UTF-16 offset，避免中文、emoji 的索引被当成字节。

离屏调用需在构建第一帧前执行 `surface.configure_text(&mut text_context)`；
`SkiaSurface` 和 `SkiaGpuSurface` 都提供此接口。不安装时，已有 GlyphRun
consumer 仍可用于 Parley/Vello 对照。Skia 按 Paragraph visitor 的实际字体、
字形位置及 glyph bounds 缓存 ink bounds，damage 同时包含新旧文字范围。

编辑器、终端、内置可编辑输入等 renderer widget 的编辑模型/绘制契约独立，
本次不把它们的 Parley/editor 状态机转换成 SkParagraph；它们仍可发 GlyphRun。
保留该入口不等于普通 text 节点还经过 Parley。测试要求普通 Skia text 的
Parley layout/glyph cache 为零，并验证完整 Taffy→Scene→Skia 路径。

验证：Skia 段落测试覆盖中英文/emoji 命中、跨段拖选与复制、应用字体注册、
换行省略、缓存复用、不同缩放下文字移动/移除的精确 damage 对照。
Lyrune 离屏歌词场景（真实封面、1.75 倍缩放、Intel Vulkan）：600 帧局部绘制
与全量 GPU 结果逐像素一致；原生 Xvfb 歌词页启动冒烟 12 秒无 panic，
日志确认选择 SkParagraph（不作长时间播放验收）。10,000 帧中 9816 帧局部绘制，RSS 从约 141 MiB
预热到 156 MiB，后段基本稳定。它不代表原生窗口或所有设备的内存上限。
Lyrune `lyric_damage` 默认选 SkParagraph，`--parley` 可保留旧路径对照；
来源缓存压力选项仅用于 `--parley` 模式。

## 运行

```sh
cargo check -p plico-skia
cargo test -p plico-skia --features raster
cargo run -p plico-skia --features raster --example compare
cargo clippy -p plico-skia --all-targets --all-features -- -D warnings
# 同时生成 Skia CPU、Skia Vulkan 和 Vello GPU 对照
cargo run -p plico-skia --features vulkan --example compare
# 明确执行需要 Vulkan 的生命周期测试
cargo test -p plico-skia --features vulkan --test vulkan -- --ignored --test-threads=1
```

示例默认使用 Linux 常见的 DejaVu Sans，并在存在时注册 Noto Sans CJK Regular。
其他系统可设置 `PLICO_PROBE_FONT=/path/to/static-font.ttf` 和
`PLICO_PROBE_CJK_FONT=/path/to/cjk-font.ttc`；显式指定的文件不存在会报错，
样本出现缺字也会报错。中文字体不可用且未显式指定时，示例诊断并跳过中文样本。

输出在 `reports/skia/`，不参与提交：

- `skia-{scale}.png`、`vello-{scale}.png`：独立的原尺寸截图。
- `compare-{scale}.png`：左 Skia CPU、右 Vello GPU，不缩放拼接。
- `skia-gpu-{scale}.png`、`compare-gpu-{scale}.png`：启用 vulkan 后额外生成，
  对照图左 Skia Vulkan、右 Vello GPU。
- scale 包括 1、1.25、1.5、2；英文含 12/14/18/32/64px 的 S/s 和数字，
  中文含 14/24px 样本，另有圆角形状及带 clip 的亚像素细线。

已在本机生成这些输出，支持检查两端的光栅化、hinting 和覆盖率差异。
相同 glyph/位置并不强制两端使用相同光栅算法，像素不同本身不等于 Vello 缺陷。
CPU Skia 对 GPU Vello 的截图不用于判断性能胜负；GPU 对照也尚未包含公平的帧耗时基准。
示例输出像素不同的数量仅用于确认路径差异，不是质量分数。

## Vulkan GPU 路径

`SkiaGpuSurface` 自己持有 Vulkan loader、instance、逻辑设备、graphics queue 和
Skia Ganesh DirectContext，直接回放指令到 GPU render target。没有 CPU scene
光栅化后再上传的中间路径。它与当前 wgpu/Vello 使用独立的设备和 context，
不做跨设备纹理共享；离屏对象与窗口对象分别管理自己的 target。

`draw_scene()` 绘制并提交，不读回像素，也不等待 CPU；`read_pixels()` 与
`write_png()` 是显式同步的截图边界。`resize()` 只更换 GPU target，保留设备、
context 与字体缓存；分配失败保留旧 target。对象不可跨线程，且不导出可逃逸的
Skia surface/context：按 surface → context → Vulkan device/instance/loader 的顺序
释放，析构等待未完成 GPU 工作。初始化失败也会清理已创建的 Vulkan 资源。

默认优先离散、集成、虚拟 GPU，最后才选择 software Vulkan；不会回退到 CPU raster。
`device_name()` 与 `is_software_device()` 报告实际设备，软件 Vulkan 不能被宣称为
硬件 GPU。示例可通过 `PLICO_SKIA_DEVICE` 名称子串选择设备，匹配失败明确报错。
Vello 的 wgpu 会自行选择 adapter；默认选择不保证与 Skia 相同。做同设备对比时显式
设置两端，例如本机已运行：

```sh
PLICO_SKIA_DEVICE='RTX 5080' WGPU_ADAPTER_NAME='RTX 5080' WGPU_BACKEND=vulkan \
  cargo run -p plico-skia --features vulkan --example compare
```


本机 RTX 5080 和 Mesa llvmpipe 均验证了连续无读回提交、RGBA 读回、clip、resize、
失败恢复与反复创建/释放；Linux CI 显式运行 llvmpipe 测试，普通测试将这些需要
驱动的测试标记为 ignored，不能只凭普通 cargo test 成功就宣称完成 GPU 验证。
当前接入要求 Vulkan 1.1，未验证 macOS/MoltenVK 或 Metal，不承诺各平台运行支持。

## 明确限制

以下情况返回 Unsupported，不静默忽略或近似替换：

- 非默认 blend mode 与图层/背景滤镜；
- 不对称端点描边、渐变及图片 brush、自定义 WGSL shader；
- SVG 的 pattern、图片、mask、filter、clip 和非默认 blend；
- 非默认 variable font 坐标、合成 italic/embolden、stroke glyph。

当前尚不能完整回放 gallery。下一阶段应逐类补齐命令和像素检查，
对同一 gallery 比较渲染质量、帧耗时、内存与平台兼容性。不能据此直接决定正式应用
的默认 renderer，更不能把尚未实现的功能记录为“已支持”。

## 实验性桌面窗口

```sh
cargo run -p plico-desktop --features skia --example skia-window
# 同一界面切回 Vello
PLICO_COMPARE_VELLO=1 cargo run -p plico-desktop --features skia --example skia-window
# 独立 X11 测试环境：鼠标、键盘、resize、隐藏恢复和关闭
python3 scripts/check-skia-window.py
```

启用 shell 的 `skia` feature 后，通过 `WindowOptions::renderer(RenderBackend::Skia)`
选择后端。窗口复用现有 Winit、输入、布局与 Parley 管线；Skia 直接绘制 Vulkan
swapchain image，通过 semaphore 同步后呈现，没有 CPU 光栅上传或每帧读回。
交换链使用 FIFO，普通帧等待前一帧 fence；resize 和析构会等待设备完成工作。
窗口格式、surface 用途或透明合成方式不满足要求时明确报错，不自动切换后端。

本机已验证 X11/llvmpipe 的交互与生命周期，并通过 Vulkan validation layer 检查；
Wayland/Intel GPU 已验证窗口初始化及呈现。其他平台尚未验证。
此例只覆盖已支持的基础控件，不代表完整 gallery 或特效支持。

### 共享 Settings 应用

```sh
cargo run -p backend-settings --features skia
cargo run -p backend-settings --features vello
python3 scripts/check-skia-settings.py
python3 scripts/check-settings-parity.py
```

三个页面直接复用同一 Settings 实现，包括 router、Details 动画、进度条、主题、
Grid、滚动列表和文字输入。Lucide 和所选 Fluent Color SVG 使用原 usvg 树转换为实色/渐变路径与描边；
转换后的指令按 document 身份缓存，源文档释放后在后续帧清理。
`check-settings-parity.py` 在同一 Xvfb 上把两个后端驱动到相同路由与主题状态，
要求同状态跨后端 RMSE 低于不同状态基线的一半——用于发现较大的视觉偏差，不证明结构完全一致或光栅化像素相同。
窗口烟测的共享机制位于 `scripts/plico_smoke.py`（Xvfb 生命周期、xdotool 输入与截图）。
深浅主题、宽窄布局及动画中间帧有独立 Skia 回放测试；窗口脚本检查真实呈现和输入。


SVG 线性/径向渐变已支持（含 stop opacity、局部 gradientTransform 与 spread），
供 Fluent Color 图标使用；当前只实现 sRGB 插值，sweep、pattern 和其他插值空间
仍返回明确错误。渐变同时用于 Skia CPU 和 Vulkan 回放。


## 文字滚动的亚像素适配

GPU surface 默认使用 `USE_DEVICE_INDEPENDENT_FONTS`，Ganesh 的
`min_distance_field_font_size` 设置为 1 个 device pixel；窗口、离屏目标和
resize 后的新 surface 使用同一策略。段落明确请求 subpixel positioning。
`PLICO_SKIA_TEXT_MODE=mask` 可在启动前恢复原有 mask 文字路径做 A/B，启动日志
记录 `text_mode`。CPU raster consumer 不改变其原有栅格化策略。

这不是帧调度优化。独立位移探针排除了输入事件、滚动插值、窗口与 damage：
文字和矩形使用完全相同的 transform，每步移动 0.125px。
原 mask 路径的文字在 0/0.125/0.25/0.375px 时像素完全相同，在 0.5px 时
整体跳到下一行像素；Vulkan 矩形却能连续移动。单纯关闭 hinting 未解决。
Skia 的 [baseline snap](https://api.skia.org/classSkFont.html) 定义了轴对齐时的
基线像素吸附；其官方 viewer 也提供
[device-independent / DFT 模式](https://github.com/google/skia/blob/main/tools/viewer/Viewer.cpp)。

仅开启 surface flag 时 28px 字体恢复连续运动，12/16px 仍因默认小字号阈值
回退到 mask。降低阈值后，12/16/28px 的九个垂直位置都产生不同像素；
按黑色像素质量计算的质心单步变化约 0.09–0.16px，没有原先的整像素跳动。
这保留 SkParagraph 的整形/断行/选区，但 distance-field 与 hinted mask 的
小字抗锯齿外观不完全一样，因此保留 mask 对照入口。

```sh
cargo run -p plico-skia --features vulkan --example text_motion
PLICO_SKIA_TEXT_MODE=mask cargo run -p plico-skia --features vulkan --example text_motion
cargo test -p plico-skia --features vulkan --test vulkan fractional_text_scroll -- --ignored --test-threads=1
```

探针输出 CSV，并把静态字号对照 PNG 写入被忽略的 reports 目录。
GPU 回归测试检查 12/16/28px 逐帧变化、质心单调小步移动以及 resize 后策略
保持。该测试默认 ignored，不增加 CI GPU 执行。它验证了文字量化问题；
没有据此证明原生窗口的帧间隔完全均匀，也没有修改 FIFO 或滚动曲线。

## GPU 缓存预算与闲置清理

Skia Vulkan 的窗口和离屏 context 默认将 GPU 资源缓存预算设置为 **128MiB**。
启动前可用 `PLICO_SKIA_GPU_CACHE_MB=64` / `128` / `256` 对照；单位为 MiB，
必须是正整数且换算后的字节数可由平台表示，非法值返回明确启动错误。
窗口启动日志包含实际预算 `gpu_cache_limit_bytes`。

自定义宿主可调用 `SkiaGpuSurface` / `SkiaWindow` 的：

- `set_gpu_cache_limit(bytes)`：软预算；持有或仍在使用的资源可以超过预算，
  不包含所有驱动分配，也不保证进程 RSS 低于这个数。显式 API 允许零以请求
  最少缓存，并不意味着 renderer 不需要内存。
- `trim_gpu_cache()`：只清理 unlocked GPU 资源，不执行 CPU 同步读回或强制
  等待，不清空应用状态，不丢弃活跃 surface 的像素与 damage history。
  后续绘制可能重新创建被淘汰的纹理/字形。

shell 在收到 suspended、首次转为 occluded，以及 memory warning 事件时
调用清理；memory warning 仍同时清理文字布局缓存。不是每帧清理，也不以普通
焦点丢失或业务播放状态代替系统生命周期事件。系统是否发送 occluded 由平台
决定，清理也不代表停止动画/音频。
profiling 下 `plico::cache / gpu_cache.trim` 记录前后 cache/purgeable 字节与
预算；不能将这些值当作所有 CPU/GPU 内存。

透明图层的 `save_layer` 现在提供变换后 clip 的 bounds 作为分配提示，仍保留
原有抗锯齿 clip 和整组透明度语义。bounds 是 hint，不是新的硬裁剪。
Lyrune 第一轮测试中单独加 bounds 未带来明显内存收益，不据此宣称已减少大块
离屏分配。

手动 Vulkan `budget_and_trim` 测试检查预算设置、清理后的像素、空 damage
历史与重新绘制；`damage_window` 在 4MiB 软预算下周期性清理，并验证 180 帧
交换链局部绘制与全量 GPU 重绘一致。这些 GPU 验证保持手动运行，不进入 CI。

## 基础绘图类型的依赖边界

几何直接使用 workspace 的 `kurbo 0.13.1`；颜色、画刷、图片和混合模式直接
使用 `peniko 0.6.1`。这些与当前 Vello 解析到的是同一份 crate，不复制实现、
不增加类型包装或运行时转换。Skia 正常依赖不再通过 `vello_common` 获取它们；
Skia 测试仅用 dev-dependency 构造暂不支持的 Vello filter，验证明确报错。

这不是整个 renderer 的完全解耦：通用指令的 filter 契约、shader TextureId、
Vello paint 上传桥接仍使用 `vello_common`，保留相关直接依赖。后续若替换这些
契约，应单独设计和验证，不把它们与基础类型的 import 迁移混在一起。

## Present skip 与实验性多矩形 damage

两项可独立对照；present skip 默认开启，多矩形 damage 默认关闭：

- 比较最后一次成功显示的 scene 与背景色，在 acquire/fence wait/flush/present
  之前跳过完全相同的帧；`PLICO_SKIA_PRESENT_SKIP=0` 禁用。Shader 和 filter 不
  被视为可证明不变。不是根据某个交换链 image 的 Empty damage 跳过显示：
  A→B→A 仍需呈现，尺寸/交换链重建、窗口恢复、显式 invalidation 也强制呈现。
  历史 Scene 共享不可变的 COW 命令存储，不逐项深拷贝绘制命令。嵌套 Fragment
  内的 shader、filter 和非默认 blend 同样按保守路径处理。
- `PLICO_SKIA_DAMAGE=1 PLICO_SKIA_MULTI_RECT=1`：为每个交换链 image 独立保留
  最多八个不重叠脏矩形，合并相交/相接区域，超额时合并增加面积最少的一对。
  逐矩形设置非 AA 的整数 clip/scissor，局部清除并按原顺序回放相交命令。
  不使用一个包围大范围的复杂 clip mask，避免其 GPU 开销。

多个矩形重放时不会将同一像素混合多次。背景变化、未知/非局部效果、首次
绘制及失败恢复仍走全量；密集变化按实际脏区面积触发全量 cooldown。Tracker 还会把
与 damage 部分相交的 Paragraph、GlyphRun、Stroke 扩张到完整命令 bounds，避免
整数 clip 切开文字或 stroke 的 AA 光栅化；普通 Fill 不做这种扩张。Fragment 作为
原子边界扩张到完整 bounds，保护内部嵌套文字与描边，但不计算其内部的最小 damage。
该策略会增加覆盖率与光栅化成本，详见 performance.md 的严格像素与成本对照。显式 `draw_scene_damaged` 绕过 tracker，调用者仍须提供安全 damage。
核心 `DamageRegion` 的 Copy/单包围框 API 保持兼容；多矩形通过
`SceneDamageTracker::damage_rects()` 读取。`last_damage()` 仍是保守包围框，
实际覆盖与数量使用 `last_damage_coverage()` / `last_damage_rect_count()`。
平台 incremental-present 提示目前仍使用保守单包围框，未实现顺序容忍匹配。

`SkiaWindow::present()` 返回 true 表示帧已接受（实际显示或 unchanged skip）；
`last_present_skipped()` 区分实际提交，false 仍表示需要重试/等待窗口恢复。
自定义宿主在要求强制重新显示时调用 `invalidate_presentation()`。
shell 的 `frame.complete` 分别记录 `frame_accepted`、`presented`、
`present_skipped`。无绘制的 headless Empty damage 也不再 flush/submit。

跳过 present 就失去了 FIFO 的隐式限速，shell 因而按当前显示器刷新周期
安排下一次自动动画唤醒（取不到刷新率时使用 60Hz），不切换成忙循环。
输入和异步事件仍可立即请求帧；静态应用不因 skip 增加计时唤醒。
profiling 记录 `present.skip` / `submit.skip`，draw span 记录真实脏区覆盖和数量。

在 Lyrune checkout 中可试：

```sh
PLICO_SKIA_DAMAGE=1 PLICO_SKIA_MULTI_RECT=1 PLICO_SKIA_PRESENT_SKIP=1 cargo run --release -p lyrune-plico --features skia -- --renderer skia
```

这些是实验性开关：本地验证包括 resize、清理、显式刷新与多 image 历史，
不据此宣称所有平台的 expose/遮挡与帧节奏都已完成验收。
