# Plico

**一个尊重 Rust borrow checker 的 GUI 框架。**

[![CI](https://github.com/SunDoge/plico/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/SunDoge/plico/actions/workflows/ci.yml)

[English](README.md) · 简体中文

用普通 Rust 函数、`if`、`for` 和借用的数据写界面，在声明按钮的地方处理点击。
Plico 提供 immediate mode 的编写方式，底层保留节点、布局、文字和绘制缓存。

**Alpha / 正在准备 0.1.0。** API 仍在演进，crate 尚未发布。

## 为什么选择 Plico？

- **顺着 borrow checker 写 UI。** 声明函数和 children 闭包同步执行：借用应用数据，
  就地处理事件，返回后结束借用。不强制 Signal 包装、hook 调用顺序，
  也不要求长期保存捕获应用状态的事件闭包。
- **原生 Rust 语法，减少视觉噪声。** `column(cx).gap(12.) + |cx| { ... }`
  使用 Rust 运算符重载；自定义组件就是普通函数。`#[component]` 注入调用点身份，
  动态列表使用业务 key。`plico fmt` 尽量平铺链式调用，让嵌套层级更清楚。
- **即时声明，复用底层工作。** 业务更新重新执行声明，与保留节点比较属性；
  布局、文字和绘制分别失效。宿主滚动和纯表现动画可以不重复执行应用的 `show`，
  界面空闲时不需要持续跑帧。
- **局部状态不必全挂在 App 上。** 共享数据使用普通 struct，组件内可用 `cx.state`。
  具名的 `cx.persisted_state` 支持尽力恢复 checkpoint，也可显式导出 MessagePack
  字节用于存储。
- **缩短开发反馈。** 普通 `main.rs` + `lib.rs` 项目、按目录运行的 CLI 和原生模块热重载。
  宿主保留窗口与服务，兼容状态恢复，不兼容状态可以重置。异步工作由宿主任务承接，
  不把运行中的 Future 放进可重载 UI 模块。
- **为不同后端留出空间。** Vello 是桌面主线；可选 Skia、FLTK + Taffy、LVGL 和
  浏览器/WASM 路径验证同一套声明模型。LVGL 已在 ESP32-P4 / ESP-IDF 上验证。

## 一个小例子

组件可以直接借用调用方的状态。不保存事件回调，children 闭包也不要求 `'static`。

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
```

完整的 [counter 示例](examples/counter.rs) 可用 `cargo run --example counter` 启动。

在 `plico_dev::App::show` 中调用即可。应用状态仍是普通 Rust 数据，组件无需实现 trait。
详见[编写方式](docs/authoring.md)和[局部状态与宿主任务](docs/local-state-and-tasks.md)。

## 运行与开发

在本仓库目录运行：

```sh
cargo run --example counter
cargo run -p gallery
cargo run -p launcher
cargo run -p backend-settings

# 创建和开发普通 Rust 应用。
cargo plico new /tmp/my-plico-app
cargo plico dev /tmp/my-plico-app
cargo plico run /tmp/my-plico-app

# 开发 gallery，或检查 UI 声明的格式。
cargo plico dev apps/gallery
cargo plico fmt --check
```

Gallery 包含布局、控件、动画、2048、流式 Markdown 和应用内毛玻璃。
CLI 支持按需生成应用自有的 SVG 图标集合，普通构建不依赖图标服务，也不必打包全部图标。
详见 [CLI](docs/cli.md)、[图标](docs/icons.md)和[平台依赖](docs/native-dependencies.md)。

## 后端与边界

| 路径 | 当前范围 |
| --- | --- |
| Vello | 桌面主线，Taffy 布局、Parley 文字、输入/IME、SVG 与动画 |
| Skia | 可选 CPU/Vulkan 回放与实验性 Vulkan 窗口后端，能力尚未完全覆盖 |
| FLTK + Taffy | 实验性原生控件和共享设置页，样式与平台覆盖有限 |
| LVGL | 实验性原生控件映射、SDL2/离屏验证，以及 ESP32-P4 / ESP-IDF 上板测试 |
| Web / WASM | 浏览器宿主与 host/UI 分离加载实验 |

Plico **不是细粒度响应式 runtime**。业务声明仍会执行视图代码并协调节点，
整条链路不保证 O(变化量)。缓存与宿主动效在保留这一简单模型的同时减少重复工作。
热重载是 best effort，不保证任意 Rust 状态无损迁移。各后端尚未完全对齐，
Bevy 和 `no_std` 尚未接入。

## 项目结构

框架库在 `crates/`，使用 `plico-` 前缀；完整应用在 `apps/`，不带前缀。

| 领域 | Package / 目录 |
| --- | --- |
| 与渲染无关的声明核心 | `plico-core`：Engine、UiContext、控件、身份与局部状态，不依赖窗口、布局引擎或渲染器 |
| 运行时与桌面宿主 | `plico-runtime`、`plico-vello` |
| 渲染与平台集成 | `plico-renderer`、`plico-shell`、`plico-motion`、`plico-native` |
| 可选后端 | `plico-skia`、`plico-fltk`、`plico-lvgl`、`plico-web-host` |
| 开发工具 | `plico-cli`、`plico-dev`、`plico-macros` |
| 富内容 | `plico-markdown`、`plico-editor`、`plico-terminal` |
| 示例应用 | `apps/gallery`、`backend-settings`、`launcher`、`terminal`、`services-demo`、`web-demo`、`lvgl-esp32p4` |

复用的 Wabou 代码在 Plico 内维护，正常构建无需原 Wabou checkout。

[文档索引](docs/README.md) · [实现状态](docs/status.md) ·
[架构](docs/architecture.md) · [贡献指南](docs/contributing.md)
