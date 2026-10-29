# 增量执行

Source 将业务声明、后端同步、布局、flatten 和 Scene 编码分别处理。普通 immediate 应用由最后一次成功提交的 kind、文字 revision 和 style 形成 dirty journal；Regions 另外把局部 replay 产生的真实节点变化送入同一 Adapter。声明失败后的恢复强制完整同步，避免遗漏中间状态。

Adapter 在结构稳定时只同步 journal、活跃动画 owner、hover/pressed/focus 的新旧节点；主题变化保守失效全部节点。paint-only 更新直接 patch 已有 placed nodes，不运行 Taffy layout 或 flatten。

## Reactive 几何与 Scene（2026-10-29）

几何变化仍从 root 调用 Taffy，保留约束求解和 Taffy 自身的干净子树缓存；本轮没有宣称布局求解已经增量。Adapter 缓存每个 retained 节点的上一轮 `taffy::Layout`，从实际 geometry dirty 节点向上检查祖先并检查沿途兄弟，只对布局确实变化的 topmost 子树重新 flatten 和 splice。未放置的零尺寸节点提升到最近的已放置祖先。结构变化、resize、主题或无法确认安全上下文时仍完整 flatten。

Regions 的真实区域边界同时作为 Scene fragment 缓存边界。干净区域以 `PaintCommand::Fragment` 引用上一帧 Scene，不再把全部命令逐条复制到根 Scene；脏区域及其缓存祖先重新编码。缓存键包含父 transform、继承 clip、viewport 和 device scale；真实拓扑变化会清缓存，避免 NodeId 复用引用旧 Scene。backdrop blur 会读取之前的 Scene 内容，因此相关子树保持 inline，不独立缓存。普通全量编码、Skia/Web 消费和正确性 oracle 使用同一命令模型；`Scene::flattened()` 递归展开 fragment 后可逐命令比较。

Seeded Reactive 差分、完整 Settings 的路由/输入/Activity reverse/主题/动画/resize 测试，每帧都将增量 Scene 展开后与同一 Source、同一 TextContext 的全量 reference 比较。renderer 另有 transform/clip/opacity layer、继承上下文、嵌套 fragment 和 backdrop 回归。强制全量 geometry/Scene 开关保留为对照路径。

`preferences_scale` 可用 `--scene` 纳入 Scene 编码，并分别用 `--full-geometry`、`--full-scene` 强制旧路径。2026-10-29 在同机 release、固定 CPU 7、8 panels、每帧一个目标、预热 200 次/测量 1000 次下，将三种模式交错运行三轮，取 p50/p95 的中位数：

| 场景 | 模式 | p50 | p95 | Scene 均值 | flatten visits | fragment build/reuse |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| typing | 全量 geometry + 全量 Scene | 145.20µs | 149.50µs | 94.02µs | 230 | 0 / 0 |
| typing | 局部 geometry + 全量 Scene | 131.74µs | 135.13µs | 93.99µs | 1 | 0 / 0 |
| typing | 局部 geometry + fragment Scene | 44.90µs | 46.77µs | 8.86µs | 1 | 3 / 12 |
| save | 全量 geometry + 全量 Scene | 162.13µs | 168.14µs | 93.24µs | 230 | 0 / 0 |
| save | 局部 geometry + fragment Scene | 58.24µs | 60.93µs | 7.98µs | 0 | 2 / 13 |
| notifications | 全量 geometry + 全量 Scene | 101.75µs | 104.41µs | 94.15µs | 0 | 0 / 0 |
| notifications | 局部 geometry + fragment Scene | 13.87µs | 14.77µs | 7.72µs | 0 | 3 / 12 |

这些数字只覆盖语义事件到 CPU 声明、同步、布局/flatten 和 Scene 编码，不包含 GPU、present、窗口或 input-to-photon 延迟。typing 仍有一次 root Taffy layout；结构、主题等场景会合理回退，不能从局部帧推导所有交互都获得同倍率收益。

## 既有 incremental 示例

```sh
cargo run -p plico-desktop --example incremental
```

`reports/incremental/report.json` 的历史实验使用模拟时钟比较 1000 个文本节点中的单个 opacity 动画和 width 动画。该示例刻意使用拥有所有权的快照接口；实际原生路径通过 `render_retained_frame` 直接消费保留节点。Scene 快照使用 COW 共享命令，修改被共享的快照时才复制，reset 不先复制旧内容。历史报告的具体数字应结合其对应版本阅读，不代表上述 Regions fragment 路径。

更多缓存与输出路径实验见 [cache.md](cache.md)。Reactive 属性 API 支持 `.rx(state, callback)` 和 `.rx((a, b), callback)`，见 [Regions](regions.md#text-属性级绑定)。
