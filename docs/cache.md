# 缓存与输出路径优化


节点存储已经使用 SlotMap；本轮优化针对实际的输出克隆与快照开销。没有改变 immediate 声明模型，也没有把所有缓存改成一种容器。

## 实现

1. Scene.commands 从 Vec 改为 `CommandBuffer(Arc<Vec<PaintCommand>>)`。Scene.clone 和主题快照共享不可变命令；push/extend 等写入通过 COW 保留快照隔离。clear/reset 遇到共享存储直接丢弃引用，避免“先复制全部命令再清空”。唯一持有者清空时仍复用容量。读取、索引和迭代保持常见 Vec 用法；显式 Vec 赋值需要 `.into()`，需要拥有独立命令时使用 `.to_vec()`。
2. FrameSource 增加可选 `render_retained_frame`：直接准备布局、编码并合成主题层，返回节点数和 build/scene 毫秒数。默认 None 保留原来的 build_frame/paint_scene/debug 路径。内置原生 shell 与开发宿主 PNG capture 已接入；保留 retained.cpu 遥测阶段与帧统计。
3. Plico 准备事件目标、推进动画、同步 backend 时不再顺便导出 Vec。真正调用兼容 build_frame 时才生成拥有所有权的快照。BackendStats.placed_node_clones 记录 Plico 导出时实际复制的节点数；不统计调用方自行 clone 的结果。
4. 主树和 Tooltip 分别借用为绘制根层，renderer 编码时顺序遍历，无需拼接复制。shader 的 DPI 修正也直接读取当前保留节点，避免 paint 阶段再复制一次整树。

旧的 build_frame / Adapter.layout 仍返回拥有所有权的 Vec，适合测试、检查与需要快照的调用者；它们没有伪装成零拷贝。原生 fast path 包含 paint_debug_overlay 的主题合成，调用方不应重复合成。

## 测量

```sh
cargo run -p plico-desktop --example cache
cargo run -p plico-desktop --example incremental
```

reports/cache/report.json 使用 1000 个文本节点、每种路径 80 个模拟时钟帧。下面是同一次本机 CPU 样本的中位数，包含准备和 scene 编码，不含 GPU、present、窗口事件或 FPS。两条路径均已使用共享 Scene；它们的差别是是否导出 placed 快照。

| 情况 | snapshot 路径 | retained 路径 | 节点导出克隆（80 帧） |
|---|---:|---:|---:|
| 空闲采样 | 292µs | 219µs | 80,160 → 0 |
| 单节点 opacity 动画 | 604µs | 535µs | 80,160 → 0 |
| 单节点 width 动画 | 1,214µs | 1,107µs | 80,160 → 0 |

同报告还对 1001 条命令各测 200 次：复现旧 Vec 深拷贝操作的中位数为 55,343ns，共享 Scene 快照为 21ns。这是一个局部操作的对比，不能作为整帧或整个 GUI 的加速倍数。

reports/cache/before/report.json 是修改前运行原 incremental 示例的直接结果，after/report.json 是修改后的同一示例。它们只有单次各阶段样本，用于保留历史证据，不据此声称稳定百分比。

## 验证与边界

- renderer 单测验证共享命令指针、写入隔离、共享 reset 不复制、消费快照不破坏另一份内容。
- Plico incremental 测试断言 native 路径与事件准备导出克隆为 0，活跃 opacity 只同步一个节点、不运行主树布局，旧 Scene 仍独立可用。
- retained_frame.rs 对 Controls/Motion 的两种路径逐像素比较，包含 Tooltip、WGSL、主题转场开始/中间/结束。
- 完整 workspace 337 项测试、严格 Clippy、rustfmt 通过。真实 CLI/cdylib 重载脚本继续验证全部浮层、标量、WGSL 与 Tooltip；Warden 的现有 native client 仍可编译。

当前 paint 变化仍完整编码 Scene；布局变化仍重新 flatten。共享命令上的写入/追加可能触发整份 COW 复制，主题转场合成也没有因此变成 O(1)。这轮减少的是不必要的输出复制与不可变快照深拷贝，尚未实现按节点/子树缓存绘制片段、局部命令重编码或完整 O(活跃节点) 的渲染管线。UI 函数执行和 core 协调仍有自身开销。
