# Custom WGSL surfaces

Plico 的 shader 是普通声明叶子，使用已内置的 Wabou GPU 管线，不通过 JavaScript、图片预生成或软件颜色模拟。

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
fn wabou_effect(uv: vec2<f32>) -> vec4<f32> {
    let time = wabou.resolution_time_scale.z;
    let strength = wabou.values[0].x;
    let wave = 0.5 + 0.5 * sin(uv.x * 8.0 + time);
    return vec4<f32>(uv.x, wave * strength, 0.7, 1.0);
}
```

默认静态绘制；animated(true) 才连续请求帧。time 是时间偏移，speed 是非负播放速率。宿主按注入 Clock 累积播放时间，暂停和恢复不会跳过暂停时长；reduced-motion 策略冻结自动播放。uniform 改变不影响布局，尺寸改变正常参与布局。

## Uniform 与资源边界

普通 effect 函数共享后端 prelude：resolution_time_scale 的 xy 是物理纹理尺寸、z 是秒、w 是 DPI scale；values 是 64 个 vec4，最多 256 个标量。返回 RGBA。ShaderAsset::new / shader_asset! 使用此模式。

ShaderAsset::module(include_str!(...)) 接受包含 vs_main / fs_main 的完整模块，并沿用 Wabou 的 raw-uniform ABI：values 从 buffer 偏移 0 写入，前三个 float 总会被宿主覆盖为 physical width、physical height、time，其余布局由模块定义。它与普通函数模式的四 float header + 256 scalar payload 不同；raw values 仍最多 256 个标量。只允许 group(0) binding(0) 的 uniform buffer，不提供自定义纹理、sampler、storage buffer 或 scene/backdrop 输入。

WGSL 经 Naga 解析与接口验证，源码上限 64KiB；参数必须有限、speed 非负。每个已挂载节点保留 ShaderEffectId、有效程序和 uniform 数据。源码错误保留上一个有效程序，首次编译错误则不绘制效果；Adapter::shader_diagnostics() 提供诊断。GPU pipeline 和输出纹理由 renderer 管理、复用及回收。卸载或暂停不会继续请求 shader 动画帧。

核心只保存可序列化源码与 ShaderParams，不携带 GPU handle。ViewNode 和开发宿主 replay 已支持此载荷，修改源文件可随应用重新编译；静态资源只 include 实际使用的 WGSL 文件。控件身份稳定时，宿主播放进度和 GPU surface 可以保留。当前是 native GPU 接口，不宣称 WebGL/Bevy 后端已接好。

values(&[...]) 在应用声明时创建拥有所有权的参数 Vec；动画中间帧不执行应用声明，也不重复创建这份参数。还没有共享 uniform block API，不宣称多 surface 的所有分配已优化。

## 对照与验证

只读参考本机 Wabou 的 shader_layer.rs / renderer shader.rs，复用本仓库中已经复制的 ShaderEffectSource、PaintContext 和 GPU renderer。没有改动原 Wabou。MyGO 的 Context-first 控件及行为/样式分层继续用于组件 API，shader 不替代常规布局、文本和语义节点。

Gallery 的 Motion 页包含实时 WGSL aurora 和暂停按钮。shader 集成测试实际检查 GPU 像素，覆盖参数、虚拟时钟、暂停/恢复、reduced-motion、2× DPI、静态帧复用、完整模块、源码错误和卸载；不是仅检查 display-list 中出现字符串。

```sh
cargo run -p plico-gallery
cargo test -p plico-vello --test shader
cargo run -p plico-gallery --example capture -- --page motion --out /tmp/shader.png
```

真实 cdylib 验证复用 scripts/check-modal-reload.py：在打开的弹窗内更新持续动画的 WGSL 程序，保留应用计数并回放 uniform，再关闭并卸载。此测试也修复了 headless 激活新模块后未请求声明的问题。证据见 reports/modal/reload.json 与 reports/shaders/reload.png。

## 同一 surface 的多个场景版本

主题转场会同时绘制同一个 surface 的旧、新快照。renderer 为同一帧中参数或程序不同的版本分配独立纹理和 uniform buffer，不能仅按 surface id 共用一个目标。下一帧复用目标，未再绘制的版本在 finish_frame 清理；不变版本直接复用已绘制纹理，也不为此重新查找或编译 pipeline。

实际 GPU 回归测试先复现了“红色旧快照被绿色新参数覆盖”，再验证并排的旧／新版本、重复绘制、单版本清理后恢复，以及 Plico 主题圆形揭示的中心新色／角落旧色和最终完整新色。该修复维护在 Plico 内置后端，不修改原 Wabou checkout。
