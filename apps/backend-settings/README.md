# Shared backend settings demo

`backend-settings` 使用同一个 immediate `Settings` 应用验证桌面后端：

```sh
cargo run -p backend-settings --features wgpu
cargo run -p backend-settings --features skia
cargo run -p backend-settings --features fltk
```

- Overview：响应式指标网格、项目列表与 Details 分栏动画。
- Activity：keyed、可反转并可滚动的活动列表。
- Preferences：原生文字编辑、保存状态、主题与 Reduce motion。

应用状态是普通 Rust 字段，路由之上的 Preferences 和 Activity 状态在切换页面时保留。
所有后端共享同一 `Settings`、Router、Taffy 布局和系统文字路径；不存在另一套声明模型或对照 API。

```sh
PLICO_WGPU_DAMAGE=1 cargo run --locked -p backend-settings --features wgpu
cargo test -p backend-settings --all-targets
```

FLTK 和 Skia 的范围与限制分别见 [FLTK 实验](../../docs/fltk.md) 与
[Skia 支持](../../docs/skia.md)。本地截图、日志和性能报告写入 `reports/`，不提交仓库。
