# Skia / Vello 绘制对照实验

状态：`plico-skia` 已提供 CPU raster 和 Ganesh Vulkan 离屏 GPU consumer；
另提供实验性 Vulkan 桌面窗口后端，默认仍使用 Vello renderer。

## 已实现

同一份 `plico_renderer::Scene` 分别交给 Skia CPU 和 Vello GPU。
文字只由 Parley 整形一次：两端使用相同字体字节、TTC index、glyph ID、位置和字号。
Skia 直接绘制这些 glyph，不重新测量或整形。字体文件按 Blob ID / index 缓存，
小字号使用灰度抗锯齿、subpixel positioning 和按命令启用的 normal hinting。

支持 solid fill 路径、几何 transform、nonzero/even-odd fill、变换后的路径 clip，
对称端点描边、普通透明合成图层、高斯模糊圆角矩形 box shadow（与 Vello 的
BlurredRoundedRectangle 语义一致，sigma 即 std_dev，退化与反向矩形取绝对值），
以及普通 solid fill glyph run。每次 render 清空旧画面并重置 clip stack，
失败不返回部分像素，也不能把失败帧导出为 PNG；下一帧可以正常重试。
像素 API 返回 tightly packed premultiplied RGBA，PNG 由 Skia 编码。

默认 feature 不加载 Skia；启用 `raster` 或 `vulkan` 才使用 skia-safe。当前 Linux 验证实际下载
并链接了 rust-skia 预编译库，不是源码构建；其他 target / feature 组合是否有预编译库
取决于 rust-skia 的分发矩阵。当前 consumer 仍复用 renderer 和 vello-common 的共享
类型与依赖，不宣称已完全解耦 Vello。

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
cargo run -p plico-vello --features skia --example skia-window
# 同一界面切回 Vello
PLICO_COMPARE_VELLO=1 cargo run -p plico-vello --features skia --example skia-window
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
