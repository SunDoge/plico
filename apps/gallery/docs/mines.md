# Mines

Gallery → Experiments → Mines 是 [Graphite Mines](https://github.com/unixzii/graphite/tree/main/examples/mines) 的纯 Rust / Plico 复刻。暖纸色、鼠尾草绿、数字格配色、尺寸页比例、经典预设、预览散列和展开/连爆节奏参考 `ad648764dd7e8b7f290693bff0eec23acf0173cd` 的 `examples/mines/{setup,game,kit}.c`；MIT 许可证与 Copyright (c) 2026 Cyandev 保存在 [graphite-MIT.txt](../licenses/graphite-MIT.txt)。不依赖 Graphite、GTK、C 或其 checkout。SVG 由本例直接声明。

- Easy 9×9 / 10、Medium 16×16 / 40、Hard 30×16 / 99；也可输入 1–64 列/行及雷数。尺寸改变会更新建议雷数；手动设置雷数后保留自定义密度，并按可用格数限制。
- 首击安全，空间允许时保护整个 3×3 邻域；空白区域自动展开。右键或 F 插旗；已展开数字周围旗数相等时，点击数字连开，错误插旗会触雷。
- Tab 遍历原生输入框、方向键调整数字、Enter 开始；棋盘方向键移动、Enter/空格打开、F 插旗、R 重开、Escape 返回尺寸页。原生输入框保留系统选区、剪贴板与 IME；与原例的自绘编辑器不同。数字格支持单击连开，无须双击。
- 展开波、弹簧悬停/按压、邻近按压凹陷、触雷涟漪与连爆、胜利彩纸、结果卡片。时间来自注入的 FrameTime，`reduced_motion: true` 跳过空间动画。Ready 与胜负动画结束后不持续请求帧；Running 保持计时刷新。
- 规则状态、RNG seed 与虚拟时钟支持确定性重放。右键消费、同一目标按下/释放和取消处理属于 Plico runtime；`UiContext::secondary_clicked()` 可在任何组合控件里使用，`secondary_click` 可供所有应用的 JSON recipe 使用。

运行 Gallery：

```sh
cargo run -p gallery
```

一次编译后跑多个场景（默认桌面 wgpu）：

```sh
cargo build -p gallery
mkdir -p reports/mines
for state in setup easy playing hard compact won lost; do
  target/debug/gallery --scenario "apps/gallery/scenarios/mines-$state.json" \
    --capture "reports/mines/$state.png"
done
```

截图只写入本地 reports/。场景通过原生语义按钮、输入事件和虚拟时钟驱动，不直接修改运行中的游戏状态。胜利场景用无雷小棋盘来稳定覆盖结束动画；失败场景使用固定种子的真实雷格。

验证：

```sh
cargo test -p gallery --test mines -p plico-runtime --test secondary_click
```
