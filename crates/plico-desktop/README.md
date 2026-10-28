# plico-desktop

Plico immediate 声明模型的真实后端适配：Engine 声明和失效 journal 连接到
Taffy 布局、Parley 文字、Vello 绘制与原生窗口。依赖的 plico-renderer、
plico-shell、plico-motion 均在本仓库维护，不要求原 Wabou checkout。

应用可通过 `run(app, title)` 静态启动实现 plico_dev::App 的业务结构体，
或用 Source / View 组合自定义宿主。`hot-reload` feature 提供通用动态模块宿主。

```sh
cargo run -p gallery
cargo plico dev apps/gallery
cargo plico render --out /tmp/gallery.png
cargo test -p plico-desktop --all-features
```

原名 plico-wabou；当前 package、目录、Rust 模块和通用宿主 binary 名称均已改为
plico-desktop / plico_desktop。来源清单、许可证以及可选 Solid 对照 reference 继续保留，
不把参考应用作为正常 Rust 构建的前提。见 [后端来源](../../backend/README.md)。
