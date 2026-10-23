# Plico

使用普通 Rust 和原生 `+ |cx| { ... }` 声明界面。事件驱动重新声明，保留树比较属性，
按变化更新布局、文字和绘制缓存；动画和滚动可以由宿主独立推进。
Plico 是后续 GUI 开发的主仓库，原 Wabou 暂停开发。

```rust
use plico_core::prelude::*;

#[component]
#[rustfmt::skip]
fn counter(cx: &mut UiContext, count: &mut u32) {
    column(cx).gap(12.).padding(24.) + |cx| {
        text(cx, "Counter").font_size(28);
        text!(cx, "Count: {}", count);
        if button(cx).text("Increment").primary().clicked() {
            *count += 1;
        }
    };
}

fn main() -> Result<(), plico_core::FrameError> {
    let mut engine = plico_core::Engine::default();
    let mut count = 0;
    engine.frame(|cx| counter(cx, &mut count))?;
    Ok(())
}
```

## 运行与开发

```sh
cargo run -p plico-gallery
cargo run -p plico-launcher
cargo test --workspace --all-features --exclude plico-lvgl --exclude plico-lvgl-esp32p4

cargo plico new /tmp/my-plico-app
cargo plico dev /tmp/my-plico-app
cargo plico run /tmp/my-plico-app
cargo plico dev apps/gallery
cargo plico render --out /tmp/gallery.png
cargo plico fmt --check

# 在应用目录按需生成图标
plico icons add lucide:save lucide:search
plico icons check
```

新项目的 lib.rs 实现 `plico_dev::App`，main.rs 通过 `plico_backend` 静态启动。
开发模式保持宿主和窗口，重载 UI 动态库；兼容状态经 Serde 恢复，不兼容时重置。
默认核心不依赖布局、渲染和窗口。项目处于 alpha，尚未发布 crate。

## 目录

| 路径 | 内容 |
|---|---|
| crates/plico-core | immediate 声明、保留节点、事件、共享类型与失效标记 |
| crates/plico-backend | Taffy/Parley/Vello 后端、原生应用适配和热重载宿主 |
| crates/plico-dev | 模块协议、状态恢复、服务桥与动态库加载 |
| crates/plico-cli | usage-rs CLI、构建监听、formatter 和按需图标生成 |
| crates/plico-macros | 调用点身份与声明语法辅助 |
| crates/plico-markdown、plico-editor | 流式 Markdown 与编辑器嵌入 |
| crates/plico-web、plico-web-ui | WASM host/UI 实验和入口 |
| crates/plico-renderer、plico-shell、plico-motion、plico-terminal-widget | Plico 内维护的渲染、平台和窗口代码，不依赖原 checkout |
| apps/gallery | 控件、布局、动画、2048、流式 Markdown 和毛玻璃 |
| apps/launcher、terminal、services-demo | launcher、终端与后台服务示例 |

Rune 和 reactive Runtime/实验及其旧应用已移除；只有 immediate 声明模型。
不宣称 ESP32、no_std、FLTK 或 Bevy 已接入。

另有 [LVGL 最小后端实验](docs/lvgl.md)，通过 oxivgl 原生控件验证同一套 API 的复用。
它的 `native` feature 需要 nightly 和 SDL2，因此单独构建和验证，不代表已支持 ESP32。

详见 [编写方式](docs/authoring.md)、[CLI](docs/cli.md)、[实现状态](docs/status.md)、
[文档索引](docs/README.md)。提交遵循 [Cocogitto / Conventional Commits](docs/contributing.md)。
