# Plico 内置后端源码

从 Wabou 复制的布局、文字、绘制、窗口和相关 Rust contracts，现由 Plico 在本仓库内维护。它不是指向外部 checkout 的链接，也不要求修改原 Wabou 项目。

当前 crate、目录和 Rust 模块路径已统一使用 Plico 命名；来源清单保留复制时的上游名称与哈希，不随本地重命名改写。

Rust crate 统一位于根目录 `crates/plico-*`；本目录保留来源、许可和渲染测试字体。

## 名称映射

| 上游/旧名称 | Plico 名称 |
|---|---|
| wabou-motion | plico-motion |
| wabou-renderer | plico-renderer |
| wabou-shell | plico-shell |
| wabou-terminal-widget | plico-terminal |
| plico-wabou（适配层） | plico-desktop |

## 组成

- plico-renderer：Taffy 布局、Parley 文字、paint scene、widget 和 FrameSource。
- plico-shell：Winit 窗口、平台输入、无窗口捕获与 Vello GPU 呈现。
- plico-motion：活跃动画 arena 与补间。
- plico-terminal：按需使用的 rio-vt / PTY 终端，已移除旧 runtime/protocol 依赖，见 [终端文档](../docs/terminal.md)。
- plico-renderer::platform：平台、事件、语义 contracts 和带类型的资源句柄（原 wabou-shell-api）。

已移除旧 JS/TypeScript bindgen、QuickJS host API、二进制 UI protocol、utility-class stylesheet 编译器和旧可选 widget 实现。renderer 仍使用的 IrColor/IrLength/IrValue 位于 style_ir 模块；CPU telemetry 位于 shell 模块。未使用的生成 TS 与协议/样式 fixtures 同步删除。Plico 自己的组件、布局和动画继续通过 Rust API 声明。

## 来源和许可

来源仓库、commit 与复制前源码 SHA-256 记录在 [UPSTREAM.json](UPSTREAM.json)。来源工作区整体有其他开发中的变更，但最初复制的十个 crate 在来源 Git 中无未提交变更。迁移调整了 Cargo manifest（workspace/path 与 publish=false），并在移入根目录 crates/ 时调整了测试数据相对路径，没有改变渲染算法。

保留 [Apache-2.0 许可证](LICENSE)。测试字体的独立许可位于 [FONT-LICENSE.txt](examples/web-preview/public/FONT-LICENSE.txt)。后续对复制代码的修改由 Plico 自行维护，不承诺自动同步上游。

## 验证

```sh
cargo test --workspace
cargo check --workspace --all-targets
cargo plico render --out /tmp/gallery.png
```

源自上游的测试保持独立可运行。实际绘制变更应先定位布局、文字、paint 或平台层，再选择相应验证；编译成功不证明视觉正确。本机离屏 1× 截图不能代替真实平台 IME、macOS 或 HiDPI 验证。

终端的后续复制有独立来源清单 [TERMINAL-UPSTREAM.json](TERMINAL-UPSTREAM.json)，不覆盖首次后端迁移的记录。
