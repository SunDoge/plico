# Layout / style 目标声明与过渡


应用声明目标值，宿主持有动画。普通 Rust 状态和短期借用保持不变：

```rust
(ui.column()
    .width(if expanded { 480. } else { 240. })
    .height(if expanded { 64. } else { 40. })
    .padding(if expanded { 16 } else { 8 })
    .gap(if expanded { 12 } else { 4 })
    .radius(if expanded { 20. } else { 6. })
    .opacity(if expanded { 1. } else { 0.75 })
    .background(Color::Literal(if expanded { 0x6352c7 } else { 0x2266bb }))
    .transition(280)).children(|ui| {
        ui.text("Layout + style").color(Color::Literal(0xffffff));
})
```

Layout、Text、Button 提供 height、opacity、radius、background、transition；Text/Button 也可设置 padding。width、height 单位是逻辑像素；radius 是统一圆角；transition 参数是毫秒。transition=0（默认）立即更新。内部使用内置 plico-motion::MotionArena 的 EaseInOut tween，不保留应用闭包。

## 当前支持与边界

| 属性 | 当前处理 |
|---|---|
| 显式 width/height、gap、padding | 中间值交给 Taffy 重新布局 |
| opacity、radius、已有背景的 RGB 颜色 | paint 过渡，不主动使文字测量缓存失效 |
| font_size、foreground、控件 checked/disabled | 立即更新；未接通字体/文字颜色动画 |
| auto 与显式尺寸互换、背景 None 与 Some 互换 | 当前属性组直接跳到目标，避免把 auto/透明误当成 0px/黑色 |
| 元素首次出现 | 从目标值初始化，不做入场动画 |
| 元素卸载 | 立即移除并释放动画；没有退出动画 |
| keyed 列表重排、对齐、百分比、min/max、grid | 尚未提供自动重排动画或完整布局 API |

当前动画只覆盖这个表格中的属性，不等同于完整 CSS transition。颜色通道按 sRGB 数值插值，未做感知色彩空间插值。opacity=0 保留布局、命中与语义，不等于隐藏/禁用。

## 帧、身份与交互

动画属于 host，按 Entity 世代句柄保存。变更目标从当前采样值重新开始，不能把每次声明都当作新动画。移除节点会移除 MotionId，重新挂载不会继承旧动画。

纯动画帧不调用 View::show、不调用动态应用模块；宿主采样动画后执行 adapter sync 与 Taffy 布局。PlacedNode 同时用于绘制、命中和语义树，因此中间帧的点击范围与可见几何一致。文字可按当前可用宽度重新换行。这选择了真实重排的 CPU 成本，没有用纯视觉缩放掩盖命中/换行问题。

后端按声明 dirty journal、活跃动画以及交互失效节点进行局部同步。paint-only 更新直接 patch retained placed nodes，跳过 Taffy layout 和 flatten；结构变化完整同步，几何变化仍布局并 flatten。未变化场景可复用；发生 paint 变化仍完整编码场景。详见 [增量执行](incremental.md)。

## 时钟与宿主接口

原生 FrameSource 使用 Instant；测试/离屏实验可调用 `Source::build_frame_at(..., now)` 和 `prepare_for_event_at(..., now)`，不睡眠。事件提交目标前先更新时间基线，防止把长时间 idle 算成新动画已播放时间。动画最后一帧提交精确目标，has_anim=false 后宿主恢复休眠。

`Source::set_reduced_motion(true)` 同时关闭属性和主题过渡，并请求重声明；尚未自动读取 OS 偏好。直接修改 Source::view 的外部调用方，应调用 request_rebuild，尤其在动画进行中。UI 事件和 reload 通路会自动标记。

应用 state 通过热更新恢复，动画则继续保存在 host；不能沿用的节点因世代变化重新初始化。App::show 返回 ()；事件消费自动请求稳定化，动画播放使用独立的 ANIMATE 请求或宿主 motion 状态。外部时钟入口见 clock.md。

## 验证与运行

```sh
cargo plico dev
# 点击 Animate layout；修改宿主后需要退出重启 dev。
cargo run -p plico-vello --example layout-style
cargo test -p plico-vello --test style_motion
```

模拟时钟测试覆盖中间几何/语义/命中一致、间距和内边距、连续 retarget、卸载、结束休眠、reduced motion 和长时间 idle。核心测试覆盖显式高度缓存更新和非法属性拒绝。

示例生成 reports/layout-style 的 0/70/140/280 ms 真实 Vello PNG 和 report.json；batch.json 记录 100/1000 个同时布局动画的单次 CPU 采样，并断言纯动画帧应用调用次数不变。它不包含 GPU/呈现耗时，不能据此声称实际 FPS。

另有 development.json 记录真实 CLI 常驻宿主 + gallery cdylib 的两次展开/收起点击及动画字段传输验证；它验证开发通路，不替代真实桌面逐帧流畅度检查。

## Flex 布局展示

Layout 新增 grow/shrink、width_percent/height_percent（单位为百分数）、min_width/max_width、align、justify 和 wrap。默认不换行，列 stretch、行 center；显式 Align/Justify 映射到 Taffy。百分比 setter 与固定尺寸 setter 相互覆盖，grow 负责分配主轴剩余空间。Text/Button 的 weight 设置数值字重并使测量缓存失效。

新增布局输入纳入核心 LAYOUT journal 比较；paint-only 仍不触发布局。当前 transition 只补间原有固定尺寸/spacing/paint；百分比、grow、align、wrap、min/max 等约束变化直接交给 Taffy，不宣称所有约束均可插值。renderer-free 核心的模拟测量不是完整 flex 求解器，真实 flex 几何以 Taffy 后端为准。

内置 gallery 默认 Layout 页面展示固定两侧栏与弹性画布、对齐和换行；Controls 和 Motion 独立页面使用应用状态切换。参见 [Gallery 与路由](gallery.md)。

当前完整布局 API 与新数值通道说明见 [布局 API](layout-api.md)。Grid 模板、区域和其他离散布局配置直接切换；同单位的百分比、四边边距、basis 等数值可插值。
