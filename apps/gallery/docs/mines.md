# Mines

Gallery → Experiments → Mines 是 [Graphite Mines](https://github.com/unixzii/graphite/tree/main/examples/mines) 的纯 Rust / Plico 复刻。暖纸色、鼠尾草绿、数字格配色、尺寸页比例、经典预设、预览散列和展开/连爆节奏参考 `ad648764dd7e8b7f290693bff0eec23acf0173cd` 的 `examples/mines/{setup,game,kit}.c`；MIT 许可证与 Copyright (c) 2026 Cyandev 保存在 [graphite-MIT.txt](../licenses/graphite-MIT.txt)。不依赖 Graphite、GTK、C 或其 checkout。SVG 由本例直接声明。

- Easy 9×9 / 10、Medium 16×16 / 40、Hard 30×16 / 99；也可输入 1–64 列/行及雷数。尺寸改变会更新建议雷数；手动设置雷数后保留自定义密度，并按可用格数限制。
- 首击安全，空间允许时保护整个 3×3 邻域；空白区域自动展开。右键或 F 插旗；已展开数字周围旗数相等时，点击数字连开，错误插旗会触雷。
- Tab 遍历原生输入框、方向键调整数字、Enter 开始；棋盘方向键移动、Enter/空格打开、F 插旗、R 重开、Escape 返回尺寸页。原生输入框保留系统选区、剪贴板与 IME；与原例的自绘编辑器不同。数字格支持单击连开，无须双击。
- 展开波、弹簧悬停/按压、邻近按压凹陷、触雷涟漪与连爆、胜利彩纸、结果卡片。时间来自注入的 FrameTime，`reduced_motion: true` 跳过空间动画。Ready 与胜负动画结束后不持续请求帧；Running 用 `request_rebuild_at` 按整秒更新计时，不再每帧重新声明棋盘。
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


## 连续帧性能修复

初版只做了规则、交互和静态截图验证，漏掉连续帧的工作量验证。三个主要成本：

1. 波纹经过数字格时才创建字形节点，每一波都使棋盘重新布局。现在逻辑展开时一次创建，后续只改呈现属性；空白格不创建无内容的字形动画容器。
2. 每个数字连续缩放，原先会触发不同 scale 的原生文字排版与栅格化。现在使用框架拥有的目标值动画：以端点的稳定密度栅格化，逐帧只变换字形四边形；数字保留缩放，动画结束的像素与静态原生字号一致。应用只声明目标值和持续时间。
3. 默认连续圆角填充逐格生成路径位图，破坏 GPU 批处理。框架对等半径、正向等比变换的连续圆角填充使用已有零 sigma 解析轮廓；不等半径、旋转、非等比变换保留原路径。轮廓和抗锯齿由 GPU 像素测试覆盖，未关闭连续圆角。

彩纸的位置、爆炸圆环的尺寸也改为 transform，动画期间不再修改布局几何、逐粒增删节点。计时器使用组件生命周期内的 deadline；组件重新声明或移除会清理旧 deadline。桌面宿主按 deadline 设置 `WaitUntil`；纯计时器等待期间复用声明与保留场景，不维持 vsync 循环。真实动效仍可按帧采样。

`mines_probe` 走真实 retained host、原生文字与 wgpu，固定种子、1120×820、45 个 17 ms 虚拟帧；排除首帧。2026-11-09 本机 Intel ARL / Vulkan 的阶段性对照中位数（下表测于迁移框架文字缩放之前，当时使用固定位移过渡；不能视为最终版本数字）：

| 棋盘 | CPU 声明至场景，修复前 → 后 | 渲染加读回，修复前 → 后 |
| --- | --- | --- |
| 16×16 / 40 | 1.05 → 0.40 ms | 48.89 → 17.53 ms |
| 30×16 / 99 | 1.96 → 0.76 ms | 105.95 → 17.78 ms |
| 64×64 / 800 | 10.75 → 3.09 ms | 19.46 → 17.85 ms |

读回列包含 CPU scene prepare、GPU 提交/执行、同步与像素复制，不是 GPU timestamp，也不能直接视作 macOS 窗口 FPS。相同 Hard 动画样本的 draw call 从 1082 降为 62；展开波与彩纸持续动画的 layout/flatten/created/removed 均为零。静止计时的两个整秒之间 declaration/sync 为零。

```sh
cargo run -p gallery --example mines_probe
cargo test -p plico-runtime --test deadline
cargo test -p gallery --test mines
cargo test -p plico-wgpu --test continuous_shadows -- --ignored
cargo test -p gallery --test wgpu mines_reveal_reuses -- --ignored
```

最后两项需要可用 GPU 与系统字体，验证连续轮廓像素、文字缓存复用以及局部重绘与完整重绘一致。原生文字缩放、整组玻璃合成与退出子树的实现和验证边界见 [呈现生命周期](../../../docs/presentation.md)。展开与交互动画由 runtime 采样；彩纸和连爆仍由注入时钟驱动应用声明，不声称所有应用时间线都已迁移。
