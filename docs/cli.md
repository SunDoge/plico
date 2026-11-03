# Plico 开发 CLI

CLI 使用 usage-rs 和 Snafu，不链接窗口与渲染器。默认调用 PATH 中的 cargo，保留 mbx/mise；`PLICO_CARGO` 可显式覆盖。

```sh
cargo plico new /tmp/my-app
cargo plico dev /tmp/my-app
cargo plico dev apps/gallery --no-watch
cargo plico run apps/gallery --release
cargo plico capture apps/gallery --scenario apps/gallery/scenarios/controls.json --output reports/controls.png
cargo plico fmt --check
cargo plico check
cargo plico test
cargo plico clippy -- -D warnings
```

`dev` 编译并重启普通 Rust 可执行文件。应用目录推断 package；工作区根目录优先 default-members，歧义时明确指定目录或 `-p`。多 binary package 使用 package.default-run。编译失败保留旧程序；不恢复内存状态，不再加载 cdylib 或注入 ABI features。普通运行与开发使用相同默认 features。`run` 提供 release/bin/features/no-default-features 与 `--` 参数转发。

`capture` 与 `--watch`、场景 JSON、虚拟时间、应用接入方式见 [场景预览](preview.md)。`plico new` 生成静态 App 和预览入口，既能 `cargo run`，也能直接接收 `--scenario` 与 `--capture`。

`fmt` 对普通 Rust 使用 rustfmt，对 `#[component]` 的 skip 区域保持紧凑方法链；支持 --check、--width、-p、路径。`icons` 为应用按需生成图标。`bundle` 构建发布目录、资源与 Linux 桌面入口。历史 `render` / `probe` 是 gallery 专用验证工具；应用截图使用 `capture`，不会静默改用 gallery。
