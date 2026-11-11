# Shader playground

Gallery 的 **Shader playground** 页把 [aaaa-zhen/glsl-playground](https://github.com/aaaa-zhen/glsl-playground)
的四组研究适配为原生 WGSL；资源在 `plico::effects`，不依赖浏览器、WebGL 或参考仓库 checkout。

- `voice_wave()`：四条光谱声波、Lorentzian 光晕和包络；强度控制振幅。
- `fluid_dots()`：六颗液滴，保留成对融合、阻尼运动、聚拢和爆发的时间方程。
- `liquid_lens()`：实际场景背景折射、预模糊、色差、黑镜面、赤道声波、轮廓光及阴影。
  同一 SDF 支持圆角透镜、液滴、胶囊，以及梯度感知的顶部半平面融合。
- `edge_light()`：抛物线角点 SDF、双瓣边光和十一光斑颜色场。
  当前使用程序噪声，未引入参考实现的外部 3D 噪声纹理、色场纹理和调试面板。

这是效果适配，尚非完整复刻参考网页：没有上传截图功能；背景来自 Plico 节点。
透镜参数经过当前 wgpu 预模糊接口适配；边缘光的噪声与参考纹理不同。
液态搜索支持从顶部拖动、按钮或进度条；拖动时使用 `UiContext::jump` 记录速度，释放后由同一 key 的 `UiContext::spring` 接管。玻璃形状和输入框共用采样进度，输入框只在 channel settled 后挂载，避免动画中提前出现。
搜索是本地可编辑输入演示，没有远端搜索服务。暂停和系统减少动态效果会跳至目标形状。

## 应用使用

```rust
use plico::prelude::*;
let wave = plico::effects::voice_wave();
shader(cx, &wave).width_percent(100.).height(240.)
    .values(&[0.65]).animated(true).paused(paused);
```

`edge_light()` 输出预乘 RGBA，可放在真实内容之后作为覆盖层。
`liquid_lens()` 读取已绘制背景；内容与输入节点仍使用普通 Plico 控件，绘制在效果之后。
它的中心和半尺寸为 surface 比例，其余距离为逻辑像素，参数顺序见 API 文档。
预模糊最大半径 32 逻辑像素，不创建独立的应用级 GPU 缓存。

自动声波/液滴/边光使用 shader 时钟，中间帧不重新声明 UI。
交互液滴在弹簧活动期间请求声明，稳定后停止；暂停、减少动态效果不持续调度该弹簧。
单页只挂载当前选中的效果，不让所有 demo 同时持续渲染。

## 验证与场景

```sh
cargo run --locked -p gallery
cargo test --locked -p gallery --lib framework_playground_assets_validate
cargo test --locked -p plico-wgpu --features window --test playground -- --ignored --test-threads=1
cargo build --locked -p gallery
# 后续场景直接复用 executable，无需再编译。
target/debug/gallery --scenario apps/gallery/scenarios/shader-lens.json --capture reports/lens.png
```

五个 `shader-{wave,dots,lens,search,edge}.json` 场景覆盖选择项；额外场景覆盖液滴融合过渡和 2× DPI。
GPU 测试检查实际像素随时间改变、背景条纹折射、形状变化、远处背景保留及 1×/2× 坐标一致。
Linux GPU 验证不代替 macOS Metal 的本机观感和性能验证；没有为此启动 macOS CI。

来源、适配边界与许可证声明见 [资源来源](../crates/plico/assets/effects/SOURCES.md)。
