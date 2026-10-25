# 绘制阶段的 motion 与失效控制

`translate / scale / opacity` 属于呈现属性，不改变 Taffy 输入，也不改变字体大小。
位移使用逻辑像素，缩放中心为布局 border box 的中心；组合顺序是先缩放、后位移。
变换应用于整个子树，包括文字和裁剪。布局空间的 `PlacedNode.rect` 保留不变，
`Adapter::visual_bounds` 与语义树提供变换后的窗口坐标。

```rust
use plico_core::prelude::*;

#[component]
fn tile(cx: &mut UiContext, x: f32, y: f32, scale: f32) {
    (column(cx).width(80.).height(80.)
        .translate(x, y).scale(scale, scale).opacity(0.9)
        .motion(Motion::Smooth)).children(|cx| {
        text(cx, "128").font_size(36);
    })
}
```

传入目标值并设置 `motion`，由宿主时钟插值；正常的动画采样帧可以跳过应用声明。
也可以像 2048 一样由应用从注入时钟采样关键帧，直接声明 translate/scale，
不再通过反复修改 absolute 坐标、尺寸和字号来完成动画。应用仍不维护 changed。

## 失效分类

`Changes` 使用 u16 bitflags，区分事件请求与下游失效：

| bit | 含义 |
|---|---|
| VIEW | 请求同帧稳定化；不是属性 diff |
| ANIMATE | 请求后续帧；不是“重做全部工作” |
| STRUCTURE | 创建、移除或重排节点 |
| MEASURE | 文字/字体等测量输入变化 |
| LAYOUT | 布局几何变化 |
| PAINT | 绘制内容或呈现属性变化 |
| TRANSFORM | 呈现坐标变化，不代表布局变化 |
| HIT_TEST | 输入路由或命中几何变化 |
| SEMANTICS | 标签、状态或语义边界变化 |

`Style::changes_from` 负责属性分类，`Engine::node_changes()` 给出当前声明后每个节点的
最终失效记录。原 `dirty_nodes()` 保留兼容。结构、文字、主题、尺寸反馈也参与引擎的分类。
稳定化可能运行多次；单次 journal 不代表完整提交，Source 会合并各 pass 的 dirty 节点和失效。

例子：

- 只改 `selected`：SEMANTICS，不写 paint，不重排。
- 只改普通颜色或非零 opacity：PAINT，不重排。
- translate / scale：PAINT + TRANSFORM + HIT_TEST + SEMANTICS，不重测文字、不重排。
- 字号：MEASURE + LAYOUT + PAINT + SEMANTICS。
- 宽度、间距等：LAYOUT，并按实际几何更新呈现和语义。

原生组合控件可能有额外依赖，例如 toggle 的 gap 同时影响 indicator fragment，
不能因为 gap 属于布局就省掉其内部重绘。测量缓存失效与普通 paint 更新分别检查。
输入编辑器还保留自己的 buffer revision 与文本 shaping 路径，不能把这套标记误读为
已经对 caret、preedit、文本内容进行了完整的细粒度 shaping 缓存。

## 呈现几何的增量传播

宿主使用 renderer 相同的 affine 组合计算 world transform，缓存每个节点的呈现几何。
普通 transform/opacity 帧只从变化节点传播到受影响子树，重叠 dirty roots 合并；
布局/结构/滚动 flatten 后才重新构建整套呈现缓存。
命中测试反变换指针到布局空间，并检查祖先裁剪。零缩放不可逆，因此不参与指针命中。
slider 拖动、输入选择与 IME caret、tooltip 以及语义边界使用相应的呈现坐标。

浮层是一个明确的依赖例外：移动 anchor 会改变独立 portal 的摆放，需要重定位并运行
相应布局；浮层自身普通 opacity 动画不再引起这项工作。这里不承诺任意嵌套浮层零重排。

## 验证与工作量

```sh
cargo test -p plico-core --test changes
cargo test -p plico-vello --test presentation_motion
cargo run -p gallery --example motion_probe
```

2048 固定棋盘的一次合并，时钟样本见 work.json（本地生成：`../reports/presentation-motion/work.json`）：
50/150/250ms 的布局运行、Taffy style 写入和 flatten visits 均为 0。
呈现传播分别访问 12/16/6 个节点，不再访问整个约 112 节点的 gallery。
开始移动、移除 ghost、移除得分提示仍然改变结构，会运行布局，不能宣称整次操作零布局。
`text_measure_invalidations` 统计 adapter 对 Taffy 文字测量的失效，不是所有 Parley shaping 调用计数。

另有 1,000 个静态兄弟节点的测试：一个两节点子树做宿主 transform 动画，呈现传播只访问 2 个节点，
应用声明、Taffy 布局和 flatten 都保持空闲。测试同时覆盖父子变换、语义边界、移动后的点击、
祖先裁剪、零缩放、纯语义更新和动画结束停止请求帧。

这里减少的是 CPU 布局、文字缓存失效和几何传播；并不等于 GPU 独立运行，
也不等于 paint scene 编码、GPU 提交或所有应用声明都消失。

本地生成图：`../reports/presentation-motion/200.png`

## 折叠节点与绘制投影

保留树中的节点不一定进入 placed 列表：零尺寸节点与空裁剪下的后代可能被省略。局部 presentation 更新按 placed 的先序深度遍历实际绘制子树，不能沿 retained children 假定每个子节点都有绘制索引。滚动、偏移钳制和焦点显露重新 flatten 后同步重建索引。`presentation_motion_only_visits_placed_nodes_when_children_are_collapsed` 回归测试覆盖带折叠子节点的父级 transform/opacity 动画，并验证可见兄弟继续移动且不触发布局。
