# Liquid Glass

Plico owns the Rust material API, shader and elevation presets. Applications own
appearance preferences and layout; no account or password-manager code lives in
this module. Warden and Gallery consume the framework API.

## Rust material and elevation

```rust,ignore
use plico::prelude::*;
column(cx).width(300.).padding(24).gap(12).radius(20.)
    .material(LiquidGlass {
        frost: 0.42,
        tint_opacity: 0.26,
        reduce_motion: preferences.reduce_motion,
        ..Default::default()
    })
    .elevation(Elevation::PANEL)
    .children(|cx| {
        text(cx, "Sharp content over a refracted scene");
        button(cx).text("Continue").elevation(Elevation::CONTROL);
    });
```

The same material works on `row`, `column` and other Layout containers. It fills
the background before normal children, follows the final radius/corner_smoothing and establishes
a relative containing block unless the pane is already absolutely positioned.
Applications do not declare a shader child, upload uniforms or retain rendering
resources. Warden's 14 recipe captures were byte-identical after this migration.

`LiquidGlass` lives in renderer-free `plico`; it declares an ordinary Shader
node. The canonical Rust asset is `crates/plico/assets/materials/liquid_glass.wgsl`.
`LiquidGlass::validated` bounds values; non-finite values revert to defaults:
frost/tint_opacity/rim 0–1, refraction 0–4 times bevel width, saturation 0–3,
bevel 0–128 logical pixels. Tint is a theme-resolved Color. `animated` is off by
default, and `reduce_motion` stops it. `enabled: false` declares an opaque fallback
without a shader. These preference flags are passed by the app/host; the material
does not read system preferences or the system clock itself.

`Elevation::{NONE,SUBTLE,CONTROL,PANEL,FLOATING,OVERLAY}` provides contact and
ambient BoxShadow pairs. Layout and leaf/control builders accept `.elevation`;
explicit `.shadow` / `.ambient_shadow` remain available for custom designs.
Shadows paint outside the rounded surface, independent of fill transparency.
Presets resolve into ordinary paint-only styles. Frost/elevation updates do not
require layout in the framework regression fixture.

`.focus_within_border` and `Input::unstyled` let composite controls put a single
focus indication on their outer frame while retaining native editing, caret,
semantics and keyboard behavior. Built-in `search_field` now uses this pattern;
custom designs must retain a visible focus indication.

```sh
cargo run --example materials
cargo test --locked -p plico --test materials --test search_widgets
cargo test --locked -p plico-desktop --no-default-features --features wgpu \
  --test materials -- --ignored
```

The detailed-backdrop GPU regression verifies actual curved-edge displacement,
an unchanged flat center, both shadow layers reaching the renderer and identical
idle captures. The standalone example has no Warden/SDK dependency. Provenance
and MIT attribution are in `crates/plico/third_party`.

Enabled glass currently requires the wgpu WGSL backdrop path. Skia/SkSL glass,
OS compositor integration and native material calibration are not implemented.
The wgpu backdrop uses a bounded area-prefilter pyramid plus a residual blur kernel
(see below); it is not a direct copy of MyGO's separable Gaussian implementation. Opaque fallback uses ordinary primitives. Keep a small
number of chrome panes: backdrop readers still require ordered background snapshots, and
the renderer only samples already-painted content in the pane's local region.
It cannot sample another window or the desktop. Transformed/masked/partially
offscreen panes need additional coverage.

The quartic/Snell lens and smoothed normals follow MyGO (MIT attribution is in
`crates/plico-renderer/third_party`). Rust now supplies Regular/Clear light/dark presets and a prefiltered blur pyramid.
No native NSGlassEffectView bridge is provided.

## Scroll-edge and independent backdrop blur

Top/Bottom/Left/Right choose which side is strongest. The opposite side preserves
the original scene; a zero-radius, untinted uniform surface is an identity pass.
Radius is a finite logical value in 0–64, Tint in 0–1. Theme tint must be supplied
by the caller; the shader does not read application/theme state.

Rust provides `BackdropBlur`, `ScrollEdgeSide`, `backdrop_blur(cx, options)` and
`scroll_edge(cx, side, options)` in the prelude. These helpers return normal
Shader builders for size/position configuration and use the same framework
asset. Theme tint resolves at declaration time.

The APIs borrow the scroll-edge composition idea from MyGO's `ui` and
`plugins/glass`, but use Plico's area-prefiltered pyramid and bounded 5×5 residual kernel.
They do not implement automatic focus-aware material policy,
per-platform native materials or arbitrary clip masks. Position these rectangular
surfaces explicitly; only scene pixels in the effect surface are sampled. They
share the same conservative backdrop damage and clipping limits as Liquid Glass.
Rust clients can reuse `plico_renderer::shader::BACKDROP_BLUR_WGSL`; vec4 slot 0 is
`[radius, mode, tint, 0]` (mode 0 uniform, 1 top, 2 bottom, 3 left, 4 right) and
slot 1 is tint RGB. No additional Rust ABI or package dependency is required.

### Dialog panels

`modal(cx, &mut state).material(glass)` uses the shared scene-backed material on
the dialog panel, with continuous corners and overlay contact/ambient shadows.
The backdrop surface stays fixed while a separate content viewport scrolls.
Glass dialogs default to an 18% scrim; opaque fallback retains the 38% default.
`Modal::scrim_opacity` explicitly overrides that amount. Passing an application
material with `enabled: false` declares an opaque panel without a shader.

## 滚动区域裁剪回归

读取背景的材质在独立合成时继续使用祖先矩形裁剪、圆角覆盖遮罩和离屏目标原点。
结果包含已采样背景，因此按裁剪覆盖率与原像素混合，不再次以 SrcOver 叠加背景。
完全离开可见裁剪的效果不执行背景复制。

```sh
cargo test -p plico-wgpu --test effects -- --ignored
cargo test -p plico-desktop --test scroll_clip -- --ignored
```

测试将同色材质与普通填充逐像素比较，并验证真实 scroll_y 控件中的玻璃、阴影和透明分组
在连续滚动与 1／1.5／2 倍 device scale 下不会越过父滚动区域或圆角。需要可用的 wgpu adapter。

## Regular、Clear 与大面板 Clarity

Rust 的 `LiquidGlass::regular()` 与 `LiquidGlass::clear()` 参考 MyGO
`plugins/glass/glass.go` 的 `paneMaterial` / `paneTone`，通过公开材质入口给容器、按钮和对话框使用。
`Regular` 依据当前 theme surface 的亮度选明／暗色调；`Clear` 只轻度调色，内容需自行保证对比度。

```rust,ignore
column(cx).radius(24.).material(LiquidGlass {
    clarity: 0.65,
    ..LiquidGlass::regular()
}) + |cx| {
    button(cx).text("Continue").primary().material(LiquidGlass::regular());
};
```

Clarity 的尺寸响应对短边 100 DIP 以内的控件保持原材质，在 100–200 DIP 之间平滑过渡，
对 200 DIP 以上的大面板完整生效。该参数同时调整大面板的色调、模糊采样范围、bezel 和 rim。
默认 `LiquidGlass::default()` 仍为 `GlassStyle::Custom`，保留旧 Rust 材质的响应；新增参数
使用第 3 组 uniform 的 y/z/w，分别是风格、clarity 和 surface 是否偏暗。

主按钮使用更强的强调色 tint，高 tint 区间平滑增加覆盖；GPU 像素测试验证内置 Default／Light／Dark 三套主题的 on-accent 文字对比度至少 4.5:1。普通面板的 tint 响应保留。
禁用的玻璃按钮使用 fallback，保留 disabled 语义与原本的透明度，不创建 shader。
减少动态效果仍由原来的 reduce_motion 控制；这些静态 presets 本身不要求持续绘帧。

新增 Gallery 场景 `effects-regular.json`、`effects-regular-dark.json` 与 `effects-clear.json`，
提供模式切换、clarity 滑杆和普通／主操作／禁用按钮。内容底板保证不同背景上的文字对比度。
GPU 测试检查参考色调端点、深色模式、small-control clarity 不变及大面板变化。
背景现先生成受最大 sigma 限制的预滤波层级，再用 25 点残余核采样；没有声称提供 HDR 或原生系统玻璃。

## 交互玻璃与原生时钟

`regular()` / `clear()` 预设现为按钮启用 `interactive`；`default()` / Custom 仍默认关闭。
设置 `LiquidGlass { interactive: true, ..LiquidGlass::default() }` 可为既有参数开启相同反馈。
容器材质保持静态；只有按钮／button_base 的交互状态会驱动反馈。

悬停在 120 ms 内提亮 rim、tone 与 tint，按压在 150 ms 内让材质向左右各外扩 1.1 DIP、
上下各外扩 0.45 DIP，并略调圆角。字形、布局和点击区域不缩放；父级滚动裁剪仍然生效。
减少动态效果时直接到目标状态。材质拥有背景后，宿主不再在下面推断一层实色 Control fill；
键盘焦点与 disabled 处理独立保留。

这由通用 `shader(...).transition_value(index, milliseconds)` 与 `paint_outset(x,y)` 支撑，
前者用注入时钟对指定 scalar 进行 retained 过渡，后者仅拓展绘制画布（每轴 0–64 DIP）。
目标中途变化时从当前采样值继续；其他参数变化不会重启未改变的目标；结束后回到 idle。
应用不需要 per-frame changed、UI state 回调或系统 Instant。

`paint_outset` 仍受祖先裁剪。renderer 在离屏目标裁剪输入背景时保留 effect UV 到纹理 UV 的映射，
避免外扩画布或 cropped opacity target 导致采样位置偏移。这份映射在每次 GPU 编码中独立拥有，
不缓存历史 backdrop view。

Gallery 的 `effects-glass-hover.json` / `effects-glass-pressed.json` 可重建交互截图。
Preview 现在支持独立的 `pointer_down` / `pointer_up`，无需把按压伪装成一次已完成的 click。

```sh
cargo test -p plico-desktop --test shader_transition --test interactive_glass
cargo test -p plico-desktop --test interactive_glass --test materials --test scroll_clip -- --ignored
cargo test -p plico-wgpu --test effects -- --ignored
```

测试覆盖：原生输入、时钟中间帧／反向与组合目标、无重复声明／布局、idle、减少动态效果、
绘制外扩但不扩大命中、键盘焦点、父级裁剪、无实色底板，以及裁剪背景像素位置不变。

## 默认连续圆角

Rust `Style::default()` 的 `corner_smoothing` 现在为 1，普通按钮、输入、布局、图像与材质
从同一默认值取得连续圆角，不需要应用逐个开启。原有半径数值不变；半径为零仍为直角。
旧预览数据省略该字段时也采用新默认值，显式 `.corner_smoothing(0.)` 可选择圆弧。

圆形与完全圆角胶囊例外：统一半径达到短边一半时，CPU fill/border/clip 使用精确 RoundedRect 圆弧，
玻璃 WGSL 使用相同的 circular 场，保留真圆和标准胶囊，并避免为这些形状创建连续轮廓路径。
API 默认、显式选择、圆／胶囊几何相等及 GPU cramped-corner／滚动裁剪都有回归。

## 内阴影

控件与布局支持 `.inset_shadow(BoxShadow { .. })`，参数仍是逻辑像素，`blur / 2` 为高斯 sigma。
正 spread 收缩孔洞，offset 移动孔洞；孔洞完全收缩时填满 caster。内阴影在普通背景上、
子内容与边框下绘制，`no_shadow()` 同时清除内外阴影。它不增加布局尺寸或外部绘制范围。

参考 MyGO `internal/scene/scene.go` 的 inset 定义，Plico 的共享 Scene 新增原生补集 primitive；
wgpu 复用解析高斯积分，Skia 使用相反轮廓的模糊。caster clip 采用完整连续圆角；
wgpu 的孔洞和外阴影均按相同连续场积分，见下文。
材质是子级 shader 时仍按通常子内容顺序覆盖背景及内阴影；玻璃自身的内缘由材质 rim 控制。

Gallery Effects 提供 Soft inset 与 Recessed surface 示例。

```sh
cargo test -p plico --test inset_shadow
cargo test -p plico-renderer --lib scene::tests
cargo test -p plico-wgpu --test inset_shadows -- --ignored
cargo test -p plico-skia --features raster --test raster inset_shadow
```

GPU 回归覆盖 1/1.5/2 倍缩放、零及非零 blur、正负 spread、空孔洞、连续边缘外部裁剪与父级 scroll clip。
Skia 有独立补集像素检查；这些是 Linux 渲染测试，不是 macOS 原生玻璃等效性验证。

## wgpu 连续轮廓阴影

普通 Rust 控件与布局的外阴影、内阴影自动继承 `corner_smoothing`。
不需要应用再次开关；显式零保留圆弧，正圆和胶囊始终采用精确圆弧。
共享 `ShadowProfile` 保留半径、sigma 与平滑度；旧的低层 `draw_box_shadow` 调用仍表示圆弧。
当前 shadow profile 是统一半径，renderer 的混合半径节点仍按其最大半径生成 caster，
与旧规则一致；这不代表任意异形路径模糊已实现。

参考 MyGO 的 `contInset`：沿水平方向求连续场零点，再积分横向 Gaussian CDF。
连续轮廓使用 16 点 Gauss–Legendre，圆弧保留既有 32 点 midpoint；中间平滑度先混合场再解轮廓。
长边中央且四 sigma 内见不到角的像素直接使用矩形解析积分，避免整条边重复求根。
这些操作不增加离屏 layer 或历史纹理缓存。

独立回归使用 CPU 连续轮廓的折线截面与 1024 点垂直积分，对照 81 组形状／
平滑度／sigma／缩放配置、每组 64 个探针；本机最大灰度误差约 1.68/255，门槛 3/255。
另有零 blur 与填充轮廓／内外互补检查、移动裁剪与局部重绘像素相等、layer allocations=0。

```sh
cargo test -p plico-wgpu --test continuous_shadows --test shadows --test inset_shadows -- --ignored --nocapture
cargo run -p plico-wgpu --example continuous_shadow_profile
```

探针固定 13 个连续圆角表面、26 个阴影，比较时保持 fill/clip 不变，仅切换阴影 profile。
采用 `render_scene_to_view` 加 GPU 完成等待，不包含 CPU 图像回读；它仍是端到端墙钟，
不是 GPU timestamp 或真实窗口呈现延迟。Intel ARL / Vulkan 的一次本机记录中，
连续阴影 1x/2x 中位约 7.51/9.51 ms，单 pass、39 instances、26 draws、零 layer allocations。
这些数值受机器、频率与同步影响，不能外推 macOS 帧率；脚本保留以便在目标机器重测。

后续优先维护 wgpu，不以 Skia 特效对齐作为交付前置。

## 面积预滤波模糊金字塔

参考 MyGO `Renderer.readBackdrop` 的先降采样再模糊思路，wgpu 的玻璃、BackdropBlur 和
四方向 ScrollEdge 现在先生成背景的面积预滤波层级，再按物理 sigma 选取 LOD 并执行
25 点残余 Gaussian 核。层级之间使用线性插值，预滤波已贡献的方差从残余 sigma 中扣除。
这是 Plico 的多层级实现，不是逐行移植 MyGO 的两轴 Gaussian pass，亦不声称任意图案上
等同于精确 Gaussian。

普通自定义 shader 仍只复制一层背景。可通过
`ShaderAsset::new(source).prefilter_backdrop(max_sigma)` 声明最大逻辑 sigma（0–128）；
已声明读取背景的 surface 可用 `.prefilter_backdrop(max_sigma)` 缩小预算。
通用 WGSL 提供 `plico_backdrop_at_lod(uv, lod)` 和
`plico_backdrop_blur(uv, sigma_in_physical_pixels)`。旧 `plico_backdrop_at` 始终读取 level 0。
若使用低层 `ShaderEffectSource`，对应 metadata 是 `.with_backdrop_prefilter(Some(max_sigma))`。
手动绑定单层背景的宿主仍可执行同一 shader，退回原来的稀疏核。

层级数为 `ceil(log2(max_sigma * device_scale)) + 1`，受区域尺寸限制；sigma≤1 的
1x 表面不生成额外层级。玻璃按 Custom／Regular／Clear 的最大 sigma 设置预算，
ScrollEdge 使用其最大半径。层级通过 render pass 生成，不依赖 compute shader。
纹理只属于本次提交，预滤波器仅保留 pipeline/layout，不缓存旧背景。
`Renderer::batch_stats()` 可查看 `backdrop_copies`、`backdrop_mip_passes` 与
`backdrop_bytes`（本帧编码的瞬时背景 payload，非 RSS）。所有层级 payload 总和小于
两倍 base，通常约 4/3；统计同样覆盖透明子层中的背景采样。

验证覆盖 1/1.5/2x、奇数／单像素轴、premultiplied RGBA、移除后的 effect target 释放、
父级圆角裁剪与裁剪离屏目标、局部重绘、跨 2/4/8/16/32 sigma 的 LOD 连续性。
细条纹回归的均匀灰度 RMS 从约 66–127 降至 0.5–1.5/255；
阶跃边缘 sigma 4/12/32 的 Gaussian CDF RMS 分别约 0.51/0.83/1.69，最大误差低于 6/255。
它们是具体测试图案的证据，不外推为全图误差上界。

```sh
cargo test -p plico-wgpu --test backdrop_prefilter -- --ignored --skip profile_many
cargo test -p plico-wgpu --test backdrop_prefilter profile_many -- --ignored --nocapture
cargo plico capture apps/gallery --scenario apps/gallery/scenarios/effects-prefilter.json --output reports/prefilter.png
```

固定 16 个 128×64 backdrop、sigma=8 的 Intel ARL/Vulkan 全重绘探针中，
单层／预滤波中位墙钟约 10.02／13.01 ms（含 CPU 准备、GPU 同步和图像回读）；
预滤波额外 48 个 pass，payload 为 696,320 B，相比 base 524,288 B。
这是以一些工作量换取采样质量，不是性能加速宣称；实际窗口和 macOS 应在目标机重测。
Gallery Effects 现在有稀疏核与预滤波的细纹理并排对照，可由场景 JSON 重建。

## 背景依赖与同阶段分组

wgpu 的局部失效不再因存在背景采样而一律全屏更新。后端提供完整采样输入矩形，
tracker 按绘制顺序传播背景变化到后续玻璃；重放时将相交玻璃的完整输入／输出与
文字／描边的原子绘制范围展开到固定点，避免误采上一帧的玻璃、光标或标签。
完全不变的窗口帧可跳过提交，玻璃外的小变化不再重算玻璃。

同阶段、等物理尺寸的背景副本可合到一个纹理数组，并按 mip 层批量执行 compute 预滤波。
每个面板仍绑定独立的 D2 slice 与自己的 mip 数，保留原有 UV、边界和采样结果。
每块区域仍执行一次 copy，减少的是 texture allocations 与预滤波 pass，不宣称减少拷贝总字节。
中间绘制若写入后续面板的输入区（包括相邻阴影），会阻止它加入该阶段；不重排画家顺序。
每组最多 16 slices、32 MiB payload、512 instances 前瞻；没有 compute／相应设备限额时
自动保留独立路径。数组与 views 只属于本次编码，不形成历史纹理缓存。

`Renderer::force_separate_backdrops(true)` 用于像素与性能 A/B，默认启用分组。
`batch_stats()` 新增 `backdrop_textures` 与 `backdrop_shared_reads`；
`backdrop_copies` 仍是 copy 次数，`backdrop_mip_passes` 统计实际 render/compute pass。
最大 sigma 预算已按 device scale 转成物理值，含 1.5/2x 验证。
详情、测量限制及复跑命令见 [实验记录](experiments/wgpu-backdrop-architecture.md)。
