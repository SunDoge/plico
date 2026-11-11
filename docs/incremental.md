# 增量执行

Plico 的应用层只有 immediate 声明。增量工作由保留后端完成：`Engine` 比较本轮声明与
上次成功提交的 kind、文字 revision、style 和子节点顺序，生成 dirty journal；
`Source` 再把 journal 交给 Adapter、布局和场景编码。声明失败会强制结构失效，避免
遗漏中间状态。

## 当前路径

- 静态节点由调用点身份复用，动态列表由业务 key 复用。
- paint-only 更新直接 patch 已有 placed node，不运行 Taffy layout 或 flatten。
- 几何变化从实际 dirty 节点向祖先检查；安全时只 flatten topmost 变化子树，大范围
  结构变化保守回退完整 geometry/flatten。
- hover、pressed、focus、滚动 presentation 与活跃动画只标记相关 owner。
- 相同 revision、surface size 与 device scale 的完整 Scene 直接复用；变化时统一编码
  当前 retained placed tree。应用不声明 Scene cache 边界。
- wgpu/Skia 的 damage tracker 根据前后 Scene 命令计算脏区；复杂 filter、shader、
  backdrop 或未知 bounds 保守扩大或回退全量。

这套协议不承诺 Taffy 的布局求解本身是增量算法，也不承诺所有声明零分配。核心
`allocations` 示例只验证预热后若干固定场景；完整应用还包含文字、布局、场景和 GPU
资源管理。

## 缓存与正确性边界

文字 shaping、SVG、shader、语义快照、hit-testing 子树包围盒和 GPU 资源分别维护
自己的 revision。主题、viewport、device scale、字体代次或无法确认的继承上下文会
扩大失效范围。语义和 hit-testing 的缓存必须与节点删除、modal/portal、滚动和
presentation transform 一起验证。

局部 damage 的正确性以全量绘制为 oracle。Tracker 会把与脏区部分相交的文字、描边
和原子效果扩张到完整命令 bounds，避免裁断抗锯齿像素。显式
`draw_scene_damaged` 绕过自动 tracker，调用者仍负责安全边界。

## 验证

```sh
cargo test -p plico-runtime
cargo test -p plico-renderer
cargo test -p plico-wgpu
cargo run -p plico-desktop --example incremental
```

性能结论必须区分 declaration、backend commit、layout/flatten、Scene 编码、GPU 编码
和 present。历史 Regions/reactive 数据只保留在实验记录中，不描述当前运行路径。
更多结果见 [缓存与输出路径](cache.md)、[性能记录](performance.md) 和
[声明成本](immediate-diff-cost.md)。
