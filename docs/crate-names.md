# Crate 命名与迁移

框架 package 放在 `crates/`，完整示例应用放在 `apps/`。后端按实际实现命名，
宿主与可加载示例分开。框架库使用 `plico-` 前缀，`apps/` 的应用 package 和
默认可执行文件使用目录名，不带此前缀：`gallery`、`backend-settings`、`launcher`、
`services-demo`、`terminal`、`web-demo`、`lvgl-esp32p4`。

终端控件库位于 `crates/plico-terminal`，package 为 `plico-terminal`，Rust 导入为
`plico_terminal`；终端应用使用 `cargo run -p terminal`，库测试使用
`cargo test -p plico-terminal --lib`。

已有后端迁移关系：

| 旧 package / Rust 名称 | 新 package / Rust 名称 | 目录 |
| --- | --- | --- |
| plico-backend / plico_backend | plico-vello / plico_vello | crates/plico-vello |
| plico-web / plico_web | plico-web-host / plico_web_host | crates/plico-web-host |
| plico-web-ui / plico_web_ui | web-demo / web_demo | apps/web-demo |

`plico-vello` 将 Plico Engine 接到 Taffy、Parley、Vello，可由桌面或浏览器宿主使用，
因此不命名为 native。`plico-lvgl` 与它是并列后端；`plico-shell` 负责原生窗口与系统
集成，`plico-renderer` 保留共享的布局、文字和场景能力。`plico-dev` 名称暂时不变。

仓库外应用需要更新 Cargo.toml 的 dependency 名称和本地 path，并将
`plico_backend::` 改为 `plico_vello::`。例如：

```toml
[dependencies]
plico-vello = { path = "../plico/crates/plico-vello", default-features = false }
```

静态入口仍使用 `plico_vello::run`。CLI 二进制仍名为 `plico`；其生成模板、默认
热重载宿主 package/binary 已同步为 `plico-vello`。自行配置的 host package、binary
及构建脚本也需更新，没有新增旧名兼容 crate。

浏览器构建脚本位于 `crates/plico-web-host/web/`，输出目录为
`target/plico-web-host` 或 `target/plico-web-host-split`。重新执行构建脚本会生成
新的 wasm-bindgen 文件名并匹配仓库内 JS import；不要混用旧构建产物。

原 Wabou 的来源清单、原始文件哈希和许可证保留原名，不把历史来源重写成 Plico。
可选 reference 工程仍排除在 workspace 之外，正常构建不依赖它。
