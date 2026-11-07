# Custom WGSL surfaces

Plico 的 shader 是普通声明叶子。`plico-wgpu` 承载 WGSL 校验及效果管线；Skia 可使用资源附带的独立 SkSL 实现。shader 不替代布局、文字或输入节点。

## C#：直接编写与热更新 WGSL

C# 前端使用同一个 wgpu 效果管线。WGSL 源码通过 batch 传到 Rust，由 Naga 校验、wgpu 创建 GPU pipeline；修改效果无需 rustc，也无需重新编译 C#。

在应用启动时加载文件，组件声明只消费快照：

```csharp
// app 是 Application / IUiDispatcher。关闭 app 前 Dispose 文件监视器。
using var effect = new WgslFile("assets/aurora.wgsl", app, watch: true);

// 在 Show(Ui ui) 内：
if (effect.Current is { } program)
    ui.Shader(program, animated: true)
        .Uniform(0, 0.8f)
        .Uniform(1, new Vector2(0.5f, 0.5f))
        .FillWidth().Height(240);
if (effect.Error is { } error)
    ui.Text(error);
```

`Uniform` 以 **vec4 槽位**寻址，范围 0–63；标量写 x，其余分量清零，Vector2/3/4 对应写入分量。上例读取 `plico.values[0].x` 和 `plico.values[1].xy`。返回预乘 RGBA。`animated: true` 让宿主更新时间并重绘 shader，动画中间帧无需重新执行 C# 声明。

`WgslProgram` 也可由源码字符串、`Load` 或 `Embedded` 创建。`Validate()` 在无窗口、无 GPU 时进行 Naga 语法/接口校验，返回诊断或 null；它不证明某台 GPU 的 pipeline 一定创建成功。`Uniform` 用于 effect/backdrop prelude；完整 `WgslMode.Module` 使用现有 `ShaderValues` 原始布局，前三个 float 由宿主写入宽、高、时间。Backdrop 模式属于高级接口，本地子区域的采样坐标已修复并有 GPU 回归测试；任意 clip mask、变换和部分移出视口仍需补充验证。通用液态玻璃接口见 [Liquid Glass](liquid-glass.md)。

`WgslFile` 首次读取和校验同步进行，应放在启动阶段。后续监视变更以 80ms 合并保存事件，在后台读取、校验，再通过 UI dispatcher 发布。错误保留上一版通过校验的源码，首次失败时 Current 为 null；恢复后清除 Error。Revision 只在有效程序变化时增加。Dispose/新请求使过期结果失效。输入限制为严格 UTF-8、64KiB。诊断携带文件名；Naga 的位置可能包含注入 prelude 的偏移。

运行 C# Gallery 的 **Effects / Shader studio** 页，直接编辑 `apps/gallery-csharp/assets/shaders/*.wgsl` 即可。发布时文件复制到 `assets/shaders`；这些文件不触发 `.NET watch` 重编译。原生库需支持协议 revision 3（升级后重启一次）；普通 shader 编辑不改变 ABI 或协议。

## Rust 静态资源

```rust
static AURORA: ShaderAsset = plico::shader_asset!("aurora.wgsl");
shader(cx, &AURORA)
    .width_percent(100.)
    .height(160.)
    .values(&[0.8])
    .animated(true)
    .paused(self.paused);
```

```wgsl
fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let time = plico.resolution_time_scale.z;
    let strength = plico.values[0].x;
    let wave = 0.5 + 0.5 * sin(uv.x * 8.0 + time);
    return vec4<f32>(uv.x, wave * strength, 0.7, 1.0);
}
```

默认静态绘制；animated(true) 才连续请求帧。time 是时间偏移，speed 是非负播放速率。宿主按注入 Clock 累积播放时间，暂停和恢复不会跳过暂停时长；reduced-motion 策略冻结自动播放。uniform 改变不影响布局，尺寸改变正常参与布局。

## Uniform 与资源边界

普通 effect 函数共享后端 prelude：resolution_time_scale 的 xy 是物理纹理尺寸、z 是秒、w 是 DPI scale；values 是 64 个 vec4，最多 256 个标量。返回预乘 RGBA（半透明输出须将 RGB 乘以 alpha）。ShaderAsset::new / shader_asset! 使用此模式。

`shader_asset!(module, "effect.wgsl")`（或 `ShaderAsset::module(include_str!(...))`）接受包含 vs_main / fs_main 的完整模块，并使用 raw-uniform ABI：values 从 buffer 偏移 0 写入，前三个 float 总会被宿主覆盖为 physical width、physical height、time，其余布局由模块定义。它与普通函数模式的四 float header + 256 scalar payload 不同；raw values 仍最多 256 个标量。只允许 group(0) binding(0) 的 uniform buffer，不提供自定义纹理、sampler、storage buffer 或 scene/backdrop 输入。顶点入口不能要求 `@location` 顶点缓冲输入，应通过 `@builtin(vertex_index)` 生成全屏三角形；片元入口必须输出单个 `@location(0) vec4<f32>`，可包装在单字段 struct 内。多颜色目标、深度和 sample-mask 输出不属于效果接口。

完整模块的 uniform 字段（含嵌套结构、数组、向量、矩阵）仅支持 `f32`。`values` 上传的是浮点字节，`u32/i32` 字段会被提前拒绝，避免把 `1.0` 误读成整数位模式。整数用途请声明 `f32` 字段后在 WGSL 中显式转换，如 `u32(u.count)`；WGSL 的 padding/stride 仍需按实际字节槽位处理。局部整数和顶点 builtin 不受此限制。

WGSL 经 Naga 解析与接口验证，源码上限 64KiB；参数必须有限、speed 非负。每个已挂载节点保留 ShaderEffectId、有效程序和 uniform 数据。源码错误保留上一个有效程序，首次编译错误则不绘制效果；Adapter::shader_diagnostics() 提供诊断。GPU pipeline 和输出纹理由 renderer 管理、复用及回收。卸载或暂停不会继续请求 shader 动画帧。

核心只保存可序列化源码与 ShaderParams，不携带 GPU handle。Rust 的 include 静态资源修改后重新编译应用；静态资源只 include 实际使用的 WGSL 文件。控件身份稳定时，宿主播放进度和 GPU surface 可以保留。当前是 native GPU 接口，不宣称 WebGL/Bevy 后端已接好。

values(&[...]) 在应用声明时创建拥有所有权的参数 Vec；动画中间帧不执行应用声明，也不重复创建这份参数。还没有共享 uniform block API，不宣称多 surface 的所有分配已优化。

## 对照与验证

只读参考本机 Wabou 的 shader_layer.rs / renderer shader.rs，复用本仓库中已经复制的 ShaderEffectSource、PaintContext 和 GPU renderer。没有改动原 Wabou。MyGO 的 Context-first 控件及行为/样式分层继续用于组件 API，shader 不替代常规布局、文本和语义节点。

Gallery 的 Motion 页包含实时 WGSL aurora 和暂停按钮，Effects 页提供三个参考 MyGO 的 WGSL 效果研究。shader 集成测试实际检查 GPU 像素，覆盖参数、虚拟时钟、暂停/恢复、reduced-motion、2× DPI、静态帧复用、完整模块、源码错误和卸载；不是仅检查 display-list 中出现字符串。

```sh
cargo run -p gallery
cargo test --locked -p plico-desktop --no-default-features --features vello --test shader
cargo run -p gallery --example capture -- --page motion --out /tmp/shader.png
```

## wgpu 窗口验证

仓库已提供文件资源与共享窗口接入的最小示例：

```sh
cargo run --locked -p plico-desktop --no-default-features --features wgpu --example wgpu-window
```

示例通过 `shader_asset!("assets/progress.wgsl")` 嵌入 WGSL，计数按钮修改
`values[0].x`，动画按钮显式控制持续请求帧。默认静态效果不会因为 prelude 中
存在时间字段而持续唤醒。动画时间来自注入时钟，不在组件中读取系统时间。

GPU 回归测试覆盖虚拟时间、暂停、参数、预乘输出、非矩形裁剪、独立透明层、
多个快照及 damage 缓存生命周期：

```sh
cargo test --locked -p plico-renderer --lib shader::tests
cargo test --locked -p plico-wgpu --test effects -- --include-ignored
```

后一条需要可用的 GPU adapter。Naga 注册校验不代替设备管线验证：例如顶点与
片元阶段的接口不匹配仍可能由设备拒绝，renderer 将其报告为 shader 错误，
测试检查随后有效效果能够恢复绘制。未实现源码文件监听或动态热重载。

## 同一 surface 的多个场景版本

主题转场会同时绘制同一个 surface 的旧、新快照。renderer 为同一帧中参数或程序不同的版本分配独立纹理和 uniform buffer，不能仅按 surface id 共用一个目标。下一帧复用目标，未再绘制的版本在 finish_frame 清理；不变版本直接复用已绘制纹理，也不为此重新查找或编译 pipeline。

实际 GPU 回归测试先复现了“红色旧快照被绿色新参数覆盖”，再验证并排的旧／新版本、重复绘制、单版本清理后恢复，以及 Plico 主题圆形揭示的中心新色／角落旧色和最终完整新色。该修复维护在 Plico 内置后端，不修改原 Wabou checkout。

## Skia / SkSL variant

A shader asset can carry separate WGSL and SkSL implementations:

```rust
static EFFECT: plico::ShaderAsset =
    plico::shader_asset!("effect.wgsl")
        .with_sksl(include_str!("effect.sksl"));
```

WGSL defines `fn plico_effect(uv: vec2<f32>) -> vec4<f32>`;
SkSL defines `half4 plico_effect(float2 uv)`. Both receive normalized local
coordinates and the same injected clock and parameters. SkSL returns
premultiplied RGBA (multiply RGB by alpha for translucent output).

The SkSL prelude declares `uniform float4 resolution_time_scale` (physical
width, physical height, seconds, device scale), and `uniform float4 values[64]`
(up to 256 scalars, zero-filled when absent). WGSL exposes the same fields
through `plico`. These names are reserved; additional uniforms and child
shaders are not part of this interface. This is not a WGSL-to-SkSL translator.

Skia compiles RuntimeEffects on first use and reuses them across frames with
a cache limited to 64 sources per rendering resource context. Its CPU raster
and Vulkan paths consume the same effects. Invalid SkSL produces a diagnostic
error; a WGSL-only asset requires the wgpu or Vello backend. WGSL validation remains part of
asset preparation on both paths. Browser WebGL2 does not support these effects.

The gallery Motion page's Aurora effect supplies both implementations.
Tests cover time/parameter pixels, compilation errors and recovery, and
rendering the complete page with Skia at two viewport sizes.

## MyGO 效果研究（Effects 页）

`apps/gallery/src/effects.rs` 与 `apps/gallery/assets/shaders/` 下的三个
WGSL 效果参考本机 MyGO 的示例，全部通过 `ShaderEffectSource` 的 Naga
校验，并由 Effects 页实时绘制；三个效果共用一个暂停开关。

| 文件 | 参考 | 参数（`values` 顺序） | 说明 |
|---|---|---|---|
| `liquid_glass.wgsl` | `examples/liquid-glass` | `frost`、`bend`（像素）、`sheen` | 必须 `read_backdrop()`：九点模糊、圆角边缘折射、色散边缘、内圈高光与缓慢扫过的高光带 |
| `orb.wgsl` | `examples/chat-glow/orb.kage` | `energy` 0..1 | 玻璃球体：五团流动色光、rim、镜面高光与外部光晕；输出预乘 alpha |
| `wind.wgsl` | `examples/wind/wind.kage` | `glow` 0..1、`hue` | domain-warped value noise 的发光风带、漂移星点与暗角 |

差异与边界：MyGO 的效果是跨 Metal/D3D11/OpenGL/WebGL2 与 CPU 的同一份
shader 源；这里只提供 WGSL，因此仅在 native wgpu（以及支持 effect 的后端）
上绘制，Skia 需要另行提供 SkSL。`liquid_glass` 读取 backdrop，宿主对它使用
full-damage 语义；orb 与 wind 不读 backdrop。三者都是单 pass，不接收纹理
或 storage buffer。Effects 页可在无窗口下捕获：

```sh
cargo run -p gallery --example capture -- --page effects --out reports/effects.png
```

## 后续扩展边界

当前参数是最多 256 个 `f32`，不是任意 Rust struct 的内存映射。类型化参数布局、
图片／sampler 输入和多 pass 渲染图尚未实现；不能将 `.values()` 解释为这些能力。
效果输出裁剪在节点表面内，外扩阴影或 backdrop 读取也没有单独的 damage 契约。
WGSL 入口为 `plico_effect` / `plico`；历史 `wabou` 拼写已随 Vello 后端一并移除。

## 具名模块参数

完整模块现在可写 `.param("progress", 0.5).param("color", [r, g, b])`。
宿主从 WGSL 解析顶层 `f32/vec2f/vec3f/vec4f` 字段的真实偏移和分量数，
不上传 Rust struct 内存。`vec3f` 后的标量也能正确打包。重复调用同名 setter
以后一次为准；字段名、类型不匹配或覆盖宿主 width/height/time 会产生诊断。

不能与 `.values(...)` 混用。函数模式、嵌套字段、数组和矩阵仍使用原始浮点
参数接口。错误更新同时保留上一份有效程序和参数；JSON 场景兼容旧配置。
完整例子与限制见 [WGSL effects](wgsl-effects.md#named-module-parameters)。
