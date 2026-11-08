# C# MyGO 改进迁移到 Rust

2026-11-08。核对本仓库 C# 组件包、材质 API 和相关提交后，补齐 Rust 公开入口及示例。
不把本来已经共享的 Rust 后端工作重复移植，也不把迁移等同于完整 MyGO 产品对齐。

参考提交：`b23a75d`（材质与组件包）、`1fa3b3e`（复选框矢量标记居中）、
`8d94a28` / `3e2e0e9`（玻璃法线与四次曲面折射）、`6a471e3`（渐进模糊）。
C# 包本身继续保留；新 Rust 使用路径不依赖 dotnet 或 managed runtime。

## 本来已共享的能力

| 原改进 | Rust 路径与判断 |
|---|---|
| MyGO 连续圆角隐式公式 | `plico-renderer::corner_shape` 已被填充、边框、轮廓和裁剪复用；没有复制第二个 CPU 几何引擎 |
| 外阴影不污染透明内容 | `scene::draw_node_box` 的反向连续轮廓切孔已经是共享实现；Rust 的双层 elevation 继续复用它 |
| CSS/MyGO blur→sigma | BoxShadow 的 blur/2 已在 runtime adapter 中统一转换 |
| 滚动条显隐、悬停/拖动保持、闲时停帧 | `plico-runtime::scrollbar_visibility`，两种前端共用 Source/adapter |
| 静态 UI 保留、普通滚动不重复声明 | Source retained frame 与 scroll observer 已供 Rust 使用；虚拟列表仍按依赖重新声明 |
| 文字度量、字形和 SVG 缓存 | 共同使用 native text/renderer；没有移植 managed 字体或按文本字形绘制图标的 workaround |
| 复选框标记居中 | Rust `toggle::paint` 已在 indicator box 内居中缩放 SVG，尺寸来自 theme spacing，不用文字勾号 |
| 帧时钟驱动 shader | Rust shader 本来就接入 native clock；不在组件中读取 Instant，也不必每个动画帧重跑 show |
| 大列表按可见行声明 | Rust VirtualList/table 已有；没有复制 C# Table<T> 的容器实现 |
| 输入/数字/时间编辑、菜单、日期、树和工具栏 | Rust 已有对应 widgets；部分语义/键盘覆盖还比 managed 组合更完整 |

这些是架构和现有实现的核对结果，不是本轮重新跑过所有历史性能实验的结论。
C# batch/FFI、GC、Interceptors 和委托分配策略不属于普通 Rust App 的执行路径。

## 本轮补齐

| C# / 原有改进 | Rust 公共 API / 结果 |
|---|---|
| BackdropBlur | `backdrop_blur(cx, BackdropBlur { .. })`；参数类型化、theme tint、无交互 overlay |
| ScrollEdge 四方向 | `scroll_edge(cx, ScrollEdgeSide::{Top,Bottom,Left,Right}, options)`；选定边最强，对边保持原场景 |
| Foreground Blur | Layout / leaf builder `.blur(sigma)`；普通 `.backdrop_blur(sigma)` 的容器背景模糊保留 |
| NoShadow | Layout / leaf builder `.no_shadow()` 清除接触与环境阴影 |
| Info / tone 颜色 | `Color::Info`、`Theme::info`、`Tone`、`badge_tone`；明暗语义色采用 managed kit 的高对比文本配色 |
| Chip / Tag | `chip`、`tag`；调用方拥有选中/删除数据，关闭图标使用矢量 X |
| 有界分页 | `pagination`；一基页码，固定数组维护最多五个数字按钮，usize::MAX 也不按总页数分配或循环 |
| Alert / Notification | `alert`、`notification`；调用方拥有错误恢复和可见性，没有隐式 IO |
| EmptyState | `empty_state`，返回可选 action 的激活结果 |
| Skeleton | `skeleton`；shader 使用 native clock，静态选项停止动画，不执行 managed 回调 |
| 圆角对照展示 | Rust Gallery Component kit → Corners：同半径填充、描边、裁剪图案及平滑度 slider |
| 玻璃与面板轮廓一致 | typed LiquidGlass 接收容器的 corner_smoothing，WGSL 使用与共享 CPU 几何相同的隐式场；曲面角落使用其梯度法线 |
| 关闭玻璃的替代面板 | opaque fallback 也继承 corner_smoothing，不再退回另一种角形 |

`backdrop_blur.wgsl` 的唯一源移入 `crates/plico/assets/materials`。原 renderer 常量改为
转发此源，managed SDK 的 EmbeddedResource 指向同一文件，原有 logical resource 名称和
uniform 合约保留。Rust App 不依赖 renderer crate 来取得这段源码。

新 Shader 返回型 helper 已登记在 `#[component]` 的 borrowing-builder 识别路径；
直接作为语句、链式 builder 或循环中的声明都可正常使用，不让 builder 逃出临时身份作用域。
宏仍只负责组件/调用点身份，不收集依赖、不改写捕获。

## 使用

```rust,ignore
use plico::prelude::*;

column(cx).radius(24.).corner_smoothing(1.)
    .material(LiquidGlass::default()).elevation(Elevation::PANEL)
    + |cx| {
        alert(cx, "Ready", "All changes are saved.", Tone::Success);
        chip(cx, "Pinned", &mut pinned);
        pagination(cx, &mut page, 1_000_000_000);
        skeleton(cx, 200., 16., !reduce_motion);
    };

// 已绘制滚动内容 → overlay → 保持清晰的标题。
scroll_edge(cx, ScrollEdgeSide::Top, BackdropBlur {
    radius: 8., tint: Color::Panel, tint_opacity: 0.8,
}).absolute(0., 0.).width_percent(100.).height(80.);
```

已有 badge / panel / field / toolbar 等接口保持兼容，不为同一功能再造完整 C# 风格层。
新增 builder 的非法 blur 值沿用 Engine 的样式校验；typed material/blur options 的
validated 方法对非有限值使用默认值，并钳位到支持范围。

## 验证

```sh
cargo test --locked -p plico --test materials
cargo test --locked -p plico-runtime --test component_kit --test focus_chrome
cargo test --locked -p plico-desktop --no-default-features --features wgpu --test materials -- --ignored
cargo test --locked -p gallery --test component_kit --test sidebar
cargo run --locked -p gallery -- --scenario apps/gallery/scenarios/component-kit.json --capture reports/component-kit.png
cargo run --locked -p gallery -- --scenario apps/gallery/scenarios/component-kit-corners.json --capture reports/corners.png
```

本轮实际通过的关键检查：

- 普通、拥挤及中间平滑度圆角的 WGSL 隐式场与 renderer 填充几何对照，包含 1× / 2× DPI。
  只比较确定的内外像素，允许栅格器边界 AA 差异，不声称与 MyGO 完整 renderer 像素相同。
- 实际 checkerboard 在曲面边缘发生折射，平面中心不变；闲时截图重复一致。
- 四方向渐进模糊的近端发生变化，对边像素保持原值。
- 一基分页在 usize::MAX 附近前后跳转、选首页；数字按钮数量保持上限。
- 原生 chip、tag、notification、空状态 action 的交互，Gallery 切换到 Corners 后调整平滑度。
- skeleton 动画中调用 retained frame，不重跑 view；关闭动画后回到 idle。
- 四种语义文本色在默认 light/dark control 表面的对比度均不低于 4.5。
- managed-only checks 继续通过，验证共享 shader asset 搬迁不破坏原 SDK 的资源加载与序列化。

## 边界

这不是新的吞吐或启动性能排行榜；本轮没有重跑冷编译/完整应用帧率对比。
有界分页、idle、paint-only 更新与 retained declaration 的性质由回归断言验证。

启用 WGSL 的玻璃、渐进模糊和 skeleton 当前以 wgpu 为验证路径；没有新增 SkSL、
MyGO 模糊金字塔、原生 NSGlassEffectView 或系统壁纸采样。
连续阴影的内部切孔已与连续圆角统一；模糊 caster 的半影仍沿用 C# 同一后端的 rounded-box
近似，不宣称已移植 MyGO 的解析连续阴影 shader。全新的 TokenField / Outline / Chart 等，
不是先前 C# 已做的功能，仍按 [MyGO 覆盖清单](../mygo-ports.md) 的已知边界处理。
