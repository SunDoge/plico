# 布局 API：尺寸、Flex 与 Grid


运行 `cargo run -p gallery`，进入 **Layout**。页面展示命名 Grid 区域、1fr/2fr 比例、Flex basis 动画、auto margin 和实际文字 baseline；缩窄窗口可以观察约束的结果，内容区支持滚动。

## 尺寸与四边间距

`width / height / min_width / min_height / max_width / max_height` 在容器和常用叶子 builder 上统一接受数值或 `Length`。数值表示逻辑像素，`percent(50.)` 表示 50%，`AUTO` 恢复自动值。`width_percent` 和 `height_percent` 继续保留。setter 最后一次调用生效，像素和百分比不会残留互相覆盖。

```rust
use plico_core::prelude::*;

#[component]
fn card(cx: &mut UiContext) {
    (column(cx)
        .width(percent(100.)).max_width(640).min_height(AUTO)
        .aspect_ratio(2.)
        .padding_edges(Edges::new(px(12.), px(20.), px(16.), px(20.)))
        .margin_left(AUTO).margin_right(AUTO)
        .background(Color::Panel)).children(|cx| {
        text(cx, "A constrained card");
    })
}
```

`Edges::new` 顺序为 top / right / bottom / left，`Edges::all` 设置四边。支持：

- `padding`、`padding_x/y`、`padding_edges`、`padding_top/right/bottom/left`。padding 支持 px / percent，不允许 auto 或负数。
- `margin`、`margin_edges`、`margin_top/right/bottom/left`。margin 支持 auto 和负值。
- `inset`、`inset_edges`、`inset_top/right/bottom/left`，结合 `position(Position::Absolute/Relative)` 使用；inset 支持 px / percent / auto 和负值。
- `gap`、`column_gap`、`row_gap`，支持 px / percent。column gap 是横轴间隔，row gap 是纵轴间隔。
- `box_sizing(BoxSizing::ContentBox/BorderBox)`、`aspect_ratio`、布局方向 `direction(Direction::Ltr/Rtl)`。

`absolute(left, top)` 保留便利用法；设置 right / bottom 时可使用 `position` 加 inset。RTL 在这里控制布局方向，不等同于完整国际化文字方向 API。百分比 padding 遵循 Taffy/CSS，以包含块宽度作为基准。

## Flex

`row / column` 默认是 Flex 容器，保留旧写法。新增：

```rust
(row(cx).width(percent(100.)).height(100.)
    .flex_direction(FlexDirection::RowReverse)
    .flex_wrap(FlexWrap::WrapReverse)
    .row_gap(12).column_gap(8)
    .align(Align::Baseline).align_content(Justify::SpaceBetween)).children(|cx| {
    text(cx, "Title").font_size(28).flex_basis(160);
    button(cx).text("Save").margin_left(AUTO).align_self(Align::End);
})
```

支持 Row / Column 的正向和反向、NoWrap / Wrap / WrapReverse，以及 Taffy 的 Balance / BalanceReverse、`flex_line_count`。`flex_basis` 支持 px / percent / auto；`grow / shrink` 保留。新增 `align_self`，容器的 `align` 增加 Baseline；`align_content` 支持 Start / Center / End / SpaceBetween / SpaceAround / SpaceEvenly / Stretch。

文字测量通过 Parley 计算第一行和最后一行 baseline，再加上 padding/border 传给 Taffy；不再简单用文字框底部作为基线。普通 Text 的不同字号和顶部 padding 已实测对齐；原生 widget 若没有提供文字测量，仍使用 Taffy 的盒子基线回退。

## Grid

`grid(cx)` 是独立构造函数，仍可用 children 和普通函数组件，不新增 Taffy 依赖到应用。

```rust
(grid(cx).width(percent(100.)).height(300.)
    .grid_columns([Track::Px(140.), fr(1.)])
    .grid_rows([Track::Px(40.), fr(1.)])
    .grid_areas(["header header", "nav main"])
    .row_gap(8).column_gap(12)).children(|cx| {
    (row(cx).grid_area("header")).children(|cx| { text(cx, "Workspace"); })
    (column(cx).grid_area("nav")).children(|cx| { text(cx, "Navigation"); })
    (column(cx).grid_area("main")).children(|cx| { text(cx, "Content"); })
})
```

轨道支持 `Auto / MinContent / MaxContent / Px / Percent / Fr / MinMax / FitContent`：

```rust
.grid_columns([
    minmax(TrackSize::Px(160.), TrackSize::Fr(1.)),
    Track::FitContent(px(240.)),
])
```

固定数量的重复可用 `[fr(1.); 3]` 或动态 Vec。`grid_auto_rows / grid_auto_columns` 定义隐式轨道，`grid_flow` 支持 Row / Column / RowDense / ColumnDense。子项可以使用：

```rust
.column_span(2)
.row_span(2)
.grid_column(GridLine::Line(2), GridLine::Span(2))
.grid_row(GridLine::Line(1), GridLine::Line(3))
.align_self(Align::End)
.justify_self(Align::Center)
```

`justify_items` 定义 Grid 容器中子项的默认横向对齐。`grid_areas` 使用空格分隔的名字，`.` 表示空单元格；区域必须是矩形，行宽必须一致。`grid_area` 可配置布局容器，普通文字/图标可包在对应区域容器内。模板和区域都经过核心及热重载协议校验；无效模板不会发布到渲染树。当前模板总轨道数限制为 1024，区域模板最多 256×256，避免意外无限输入。

## 保留、失效和动画

新增的固定大小布局属性位于 `Style.layout`，保持 Style 为无堆分配的 Copy 值；可变长 Grid 数据放在节点独立的 `GridTemplate`。重复声明相同模板不会重复写入 retained payload 或触发布局；模板变化会增加节点布局版本并请求真实 Taffy 重排。普通组件仍无需维护 changed 或手写静态 key。

原有尺寸/spacing 动画继续有效；现在同单位的百分比宽高、min/max、四边 margin/padding/inset、横纵 gap、flex-basis 和 aspect-ratio 也纳入宿主数值动画。仅同一种单位、且存在性不变的数值可插值；px ↔ percent、auto ↔ explicit 会直接切换。颜色与 transform 仍走原有通道。

Grid 模板/区域、放置、对齐、排列方向、显示模式、box-sizing 等离散配置直接切换，不会假装存在连续轨道动画。Gallery 的 Animate layout 因此让 Grid 侧栏宽度直接切换，同时让下面的 flex-basis 平滑过渡。数值动画只由宿主注入的时钟推进，不要求保存闭包。

原生热重载与 WASM split replay 都传递 Grid payload 与布局属性；模板不是全局注册表或编译器内部句柄。应用状态仍仅序列化自身数据。

## 验证与剩余边界

`cargo test -p plico-vello --test layout_features` 用真实 Taffy/Parley 坐标验证 fr 比例、跨度、模板更新、resize、命名区域、auto margin、四边 padding、右下定位、aspect-ratio、reverse、content-box、百分比约束、隐藏节点、文字基线、column auto-flow、dense 回填、minmax、fit-content、RTL、数值动画、快照和非法区域拒绝。Web smoke 在 1×、2× 下验证 Grid 页的布局切换、窗口缩放和后续其他页面。

这还不是 Taffy 全量 API。以下尚未完整开放：Grid auto-fit/auto-fill/repeat 和任意命名网格线、calc 与尺寸的 min-content/max-content 值、四边独立 border 及其绘制、float/clear/contain、fixed 定位，以及完整独立 overflow-x/y 与横向滚动。`Display::Block/None` 已映射，但 Block 的更多特有属性还未整理为应用 API。

核心 `Engine::frame` 的无渲染模拟尺寸仍是简化估算，不是另一套完整 Flex/Grid 实现；真实应用布局使用 plico-vello 的 Taffy 路径，测试高级布局也使用该路径。
