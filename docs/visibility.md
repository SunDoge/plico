# 通用声明期裁剪

`plico::VisibleRegion` 是不依赖 renderer、窗口、Taffy 或 Reactive 的值类型，用于在
创建节点之前判断对象是否可见。它没有堆分配，也不保存用户数据或回调。

三种职责保持分离：

- `VisibleRegion`：应用只对可见内容执行声明代码。
- `.clip(true)`：最终绘制不能越过容器边界，不会自动减少已执行的声明。
- damage tracking：对上一帧的变化区域局部重绘，不负责查询数据或虚拟化。

## 图片与画布

矩形使用逻辑坐标 `[left, top, right, bottom]`。`cx.viewport_region()` 仅代表根窗口
视口，**不是当前容器的布局或继承 clip**。子画布须显式交集其窗口坐标边界，再通过
与绘制相同的 local-to-window 变换转回内容坐标：

```rust
use plico::{UiContext, VisibleRegion};

fn visible_image_area(
    cx: &UiContext<'_>,
    canvas_in_window: [f32; 4],
    image_to_window: [f32; 6],
) -> VisibleRegion {
    cx.viewport_region()
        .intersect(VisibleRegion::from_rect(canvas_in_window))
        .to_local(image_to_window)
        .overscan([16., 16.])
}
```

对于 OCR，bbox 保留原图坐标。对 `visible.intersects(bbox.rect)` 为真的对象声明节点；
缩放/平移不修改原始 bbox。变换参数为 `[a,b,c,d,tx,ty]`，含义：
`window_x = a*x + c*y + tx`，`window_y = b*x + d*y + ty`。

旋转、斜切会生成保守 AABB，允许多声明一些内容，不会只取两个对角点而漏掉可见对象。
非有限值、不可逆变换退回 `UNBOUNDED`；空视口仍为空。边界仅接触没有可见面积，返回
false；对象 bounds 须包含描边、阴影、控制点等视觉外扩，不能只传图片/正文的裸边界。

这不是空间索引：遍历所有 bbox 再判断相交仍是 O(N) 元数据查询，只减少节点声明。
已有网格/R-tree/其他索引的应用，可以用 `visible.bounds()` 查询候选项；None 表示
无界，必须保守处理。当前不自动读取上一帧节点几何，也不保证动画中间位置自动参与查询。
运动对象应传入覆盖其运动路径的 bounds，或在运动期间保留声明，防止被提前裁掉。

## 表格与网格

固定行列间距可直接计算索引范围，无需遍历全部数据：

```rust
use plico::VisibleRegion;
let visible = VisibleRegion::from_rect([200., 1000., 500., 1100.]);
let rows = visible.rows(1_000_000, 20., 2);     // 48..57，含上下 overscan
let columns = visible.columns(1000, 100., 1); // 1..6，含左右 overscan
```

行/列从内容坐标 0 开始，stride 是包含 gap 的固定间距。普通 Rust 的两层 for 仅遍历
这两个范围；声明时保留业务 key，按原始行列坐标定位。应用仍需提供完整内容尺寸/占位，
否则滚动容器只会认为可见部分就是所有内容。可变行高、空间索引、固定表头和编辑保留策略
不在这个几何查询 API 内。

`virtual_list` 已复用此范围计算，负责竖向滚动反馈、上下占位和固定行高包装；参见
[虚拟列表](virtual-list.md)。画布示例使用同一相交查询，开启声明裁剪后会随窗口大小更新：

```sh
cargo run --release -p backend-settings --features skia --example graph_declaration -- --immediate --visible-only
```

## 生命周期

未声明的节点会按正常规则卸载。需要跨滚动保留的编辑内容、OCR 结果和选择，应保存在
应用业务数据中；不能依赖被卸载节点的 local state。正在拖拽、拥有焦点或播放退出动画的
对象，可以通过 `visible.intersects(bounds) || is_active` 显式保留，不改变输入/焦点规则。
