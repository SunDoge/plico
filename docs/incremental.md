# 增量执行

Source 将业务声明、后端同步、布局、场景编码分别处理。应用仍使用 immediate API；后端保留节点和动画对象，不保留应用的声明闭包。

核心以最后一次成功提交的 kind、文字 revision 和 style 收集 dirty journal；Source 合并同一业务帧中多次稳定化声明的 journal，直到后端消费才清空。声明失败后的恢复强制完整同步，避免遗漏中间状态。STRUCTURE 标记新增、删除和顺序变化。

Adapter 在结构稳定时只同步 journal、活跃动画 owner、hover/pressed/focus 的新旧节点；主题变化保守失效全部节点。动画通过 MotionId → Entity 反向索引定位 owner，复用采样缓冲，最终采样与关闭动画后的目标值也必须提交。禁用控件先清除焦点再更新 paint。

paint-only 更新直接 patch 已有 placed nodes，不运行 Taffy layout 或 flatten。宽高、padding、gap、文字测量、viewport 或结构变化仍触发布局并重新 flatten。未改变可视 revision 和缩放的场景复用缓存；发生 paint 变化仍重新编码整份场景。

```sh
cargo run -p plico-backend --example incremental
```

reports/incremental/report.json 使用模拟时钟比较 1000 个文本节点中的单个 opacity 动画和 width 动画。纯 paint 动画中间帧只同步 1 个节点，layout_runs=0、flatten_visits=0；width 动画只同步 1 个节点，但 layout_runs=1、flatten_visits=1002。稳定帧 sync_visits=0 并复用场景。测试另覆盖稳定化 journal、失败恢复、手工声明、禁用焦点与 reduced motion。

这不是端到端 O(活跃节点) 的承诺：业务帧仍完整执行应用/核心声明，结构变化仍完整同步，几何变化仍完整 flatten；兼容的 build_frame 快照接口仍克隆完整 placed 输出；原生宿主已使用 render_retained_frame 直接消费保留节点，无需导出 Vec。Scene 快照改为 COW 共享命令，修改被共享的快照时才复制，reset 不先复制旧内容。当前属性动画的场景编码仍可能成为主要成本，后续需考虑场景片段保留与输出接口调整。计时只包含 CPU build/scene，不包含 GPU、present 或 FPS。


缓存与输出路径的后续优化、前后报告和具体成本见 [cache.md](cache.md)。原有 incremental 示例仍刻意使用拥有所有权的快照接口；实际原生路径通过可选 render_retained_frame 接口避免该导出成本。
