# 固定行高虚拟列表

Immediate `virtual_list` 接收条目数和固定行高，只声明可见行与 overscan 行。数据仍由
应用拥有，稳定业务 key 由调用方提供：

```rust
use plico::prelude::*;

fn records(cx: &mut UiContext<'_>, rows: &[(u64, String)]) {
    virtual_list(cx, rows.len(), 32.)
        .height(240.)
        .smooth_scroll(true)
        .overscan(2)
        .rows(
            |index| rows[index].0,
            |cx, index| { text(cx, &rows[index].1); },
        );
}
```

默认高度为 240、overscan 为 2；`.fill()` 使用父容器剩余高度，宿主尚未报告实际高度时
使用 `.height(...)` 的回退值。`.reveal(Some(index))` 请求显示一行，不改变键盘焦点；
应只在选择变化时传 Some，其余帧传 None，避免干扰用户滚动。

框架读取当前容器的宿主滚动反馈，使用 [VisibleRegion](visibility.md) 计算范围；顶部、
底部占位维持完整内容高度，每个挂载行有固定、不可收缩且带 clip 的外层。普通容器不读取
此反馈时，仍可只在宿主滚动；虚拟列表范围变化则需要重新声明，以挂载/卸载对应行。

屏幕外行卸载，重叠业务 key 保持节点身份；需要跨滚动保留的编辑状态必须放在应用业务
数据中，不能只依赖行内 local state。可变行高、自动测量、二维表格布局和焦点行自动保留
不属于当前 VirtualList API。大数据的过滤、排序、字符串生成也应移出每次声明路径。

这是节点数量优化，不是数据存储优化，后端仍处理当前挂载的树。基础回归位于
`crates/plico/tests/visibility.rs`，覆盖百万行范围、宿主滚动反馈，以及二维可见单元格的
节点数量和重叠业务身份；真实窗口滚动、输入和热重载行为还由宿主路径管理。
