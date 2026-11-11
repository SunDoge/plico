# 虚拟列表

Immediate `virtual_list` 接收条目数和固定行高，只声明可见行与 overscan 行。数据仍由
应用拥有，稳定业务 key 由调用方提供：

```rust
use plico::prelude::*;

fn records(cx: &mut UiContext<'_>, rows: &[(u64, String)]) {
    virtual_list(cx, rows.len(), 32.)
        .height(240.)
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

框架读取当前容器的宿主滚动反馈，使用 [VisibleRegion](visibility.md) 计算固定行范围；顶部、
底部占位维持完整内容高度，每个挂载行有固定、不可收缩且带 clip 的外层。普通容器不读取
此反馈时，仍可只在宿主滚动；虚拟列表范围变化则需要重新声明，以挂载/卸载对应行。

内容高度不固定时使用 `variable_virtual_list`：

```rust
variable_virtual_list(cx, messages.len(), 48.)
    .height(360.)
    .overscan(2)
    .rows(
        |index| messages[index].id,
        |cx, index| message_row(cx, &messages[index]),
    );
```

固定和可变列表都可为聊天、日志等追加流加 `.follow_end()`：列表初次显示末尾，用户仍在
末尾时随新增行继续跟随；可变列表也跟随已挂载行变高。用户向上滚动后停止抢占位置，滚回
末尾后自动恢复。它使用宿主实际 offset 和上一轮内容范围判断，不要求应用每帧调用
`reveal`；显式 `reveal(Some(...))` 优先并暂停跟随，直到用户重新滚回末尾。

需要平滑追加时使用 `.follow_end_with(Animation::SMOOTH)`。它复用框架的 keyed scalar
channel 和注入时钟：追加途中再次 retarget 保持连续，程序写入的中间 offset 不会被误判为
用户滚动，reduced-motion 会直接到达末尾。列表不维护私有定时器，也不会固定按 16ms 唤醒。

可变高度实现翻译自本机 MyGO `ui/list.go` 的 `listHeights`：以 64 行为稀疏块，只保存实际
挂载过的行高；未测行使用已测高度平均值，首次测量前使用构造参数中的估算值。尺寸反馈是
按需的：只有调用 `UiContext::layout_size` 的可变行容器会在 Taffy 布局后收到 border-box
尺寸，普通节点不会因几何变化重跑声明。overscan 行的实际高度修正时，列表同步修正可见行
锚点，避免内容在测量收敛时跳动。行数或顺序变化后，列表在原锚点附近最多检查 1000 行
业务 key；找到同一项时修正 offset，可变列表同时平移稀疏高度缓存，因此固定或可变列表在
头部加载旧消息都不会推动当前视图。两种列表共用同一有界 key 搜索；key 回调应是廉价、
确定且无副作用的业务 ID 查询。

固定和可变列表都可用 `rows_with_headers` 声明分区标题：

```rust
virtual_list(cx, rows.len(), 32.).rows_with_headers(
    |index| rows[index].id,
    |index| rows[index].is_header,
    |cx, index| row(cx, &rows[index]),
);
```

列表会额外挂载视口上方最近的标题，并让它吸附在滚动区域顶部；下一标题到达时会把当前
标题顶走。标题查询翻译自 MyGO `ListState.headerAbove`，向后查找最多 100,000 行并缓存
结果，不会为百万行列表无界扫描。`is_header` 与 key 回调一样应廉价、确定且无副作用；对
同一业务 key，是否为标题应保持稳定。额外挂载的标题不计入 `rendered` 可见范围，但仍有
正常节点身份、布局、绘制与命中测试。

屏幕外行卸载，重叠业务 key 保持节点身份；需要跨滚动保留的编辑状态必须放在应用业务
数据中，不能只依赖行内 local state。锚点业务项移动超过 1000 行或已被删除时，列表不会
为了找它扫描全部数据，而会保留当前绝对 offset 并让新挂载行逐步重测。二维表格布局和
焦点行自动保留不属于当前 API。大数据的过滤、排序、字符串生成应移出每次声明路径。

这是节点数量优化，不是数据存储优化，后端仍处理当前挂载的树。基础回归位于
`crates/plico/tests/visibility.rs`，覆盖百万行范围、宿主滚动反馈，以及二维可见单元格的
节点数量和重叠业务身份；`plico-runtime/tests/variable_list.rs` 通过真实 Taffy 布局验证稀疏
尺寸反馈、范围收敛、有界节点数、头部插入后的业务锚点位置，以及 follow-end 的暂停和恢复。
