# Plico 开发 CLI


`plico-cli` 使用 usage-rs 统一声明参数、帮助和校验，错误使用 Snafu。CLI 不链接 GUI/渲染依赖；help 不初始化 GPU。

```sh
cargo plico --help
cargo plico dev apps/gallery         # 从应用目录启动，监听与热更新
cargo plico new /tmp/my-plico-app     # 独立 Rust 应用（main.rs + lib.rs）
cargo plico dev /tmp/my-plico-app
cargo install --path crates/plico-cli
```

CLI 默认通过 PATH 寻找 `cargo`，因此会使用 mbx、mise 等 Cargo 包装器，也继承
Cargo 的 `rustc-wrapper` 配置。需要覆盖命令时设置 `PLICO_CARGO=/path/to/cargo`。
不使用 Cargo 自动注入的 `CARGO` 路径，以免 `cargo plico` / `cargo run` 绕过包装器。

## 普通 Rust 项目

生成的项目包含 `src/main.rs` 和 `src/lib.rs`，共享同一个 `Application: plico_dev::App` 实现：

```sh
cd /tmp/my-plico-app
cargo run                         # 静态链接应用，打开桌面窗口
cargo run --release               # 发布构建，无需 CLI 或动态模块
cargo build --lib --no-default-features --features abi # 轻量 rlib + cdylib
plico dev                         # 常驻 host + 动态库热重载
```

默认 `desktop` feature 启用可选的 `plico-vello` 依赖；二进制入口要求该 feature。`main.rs` 通过 `plico_vello::run(Application::default(), title)` 启动，应用逻辑留在 `lib.rs`。库提供 `rlib` 和 `cdylib` 两种产物；只有开发宿主使用动态加载，普通运行直接调用 App，不经过 JSON/FFI。

## 目录与运行模式

```sh
plico dev /path/to/app       # 常驻宿主 + 可重载 UI
plico run /path/to/app       # 普通静态应用，使用默认 features
plico run /path/to/app --release
cd /path/to/app
plico dev                    # 当前应用目录也可省略
plico run
```

CLI 从目录判断当前 package，workspace 根目录优先选择唯一可用的 default member，
否则选择唯一可用应用。存在多个候选时列出目录并要求指定，不再硬编码 gallery。
`-p` 保留为高级覆盖选项。`dev` 需要 cdylib target；`run` 支持普通 binary 应用，
多个二进制时可用 `--bin`，其默认选择遵循 Cargo 的 default-run。
`--` 后的参数传给应用。`-C` 仍可设置工作目录，位置参数相对于该目录解析。

| 模式 | 应用/宿主 features | UI 模块 |
|---|---|---|
| run | 应用默认 features，通常 desktop | 静态链接，不加载开发描述符 |
| dev 通用宿主 | 后端显式 hot-reload | --lib --no-default-features --features abi |
| dev 自定义宿主 | 保留应用默认 features；定义 hot-reload 时额外启用它 | --lib --no-default-features --features abi |

新模板提供 `desktop` 与 `hot-reload` feature，后者映射到后端的动态加载能力。
旧自定义宿主没有该 feature 时暂按原配置构建；建议迁移到上述约定。
`run --features ...` / `--no-default-features` 可显式覆盖正常运行的构建配置。
普通 run 会清除继承的 PLICO_DEV_MODULE，避免误用遗留开发会话。

## 开发链路

第一次构建常驻的 `plico-vello` host（不依赖 gallery），再构建轻量的 app cdylib。后续由 notify 监听 Rust 源码/Cargo.toml 变化，只重新构建 app。开发构建使用 `--lib --no-default-features --features abi`，不启用桌面后端依赖；重载不重新链接 renderer，也不重启窗口。

产物复制到本次会话的私有路径后，原子更新 active descriptor。宿主通过 Stabby 的布局报告加载导出函数，以版本化 JSON 请求/响应交换状态与 UI 声明；成功才激活，失败保留上一版。普通 Rust panic 在模块边界转换成错误并保留 panic 消息。恢复不了应用状态时重置为 Default 并报告原因；当前恢复粒度是整个 App struct，不是逐个状态槽位。

模块在 UI 线程保留 App 与 Engine：纯动画/滚动帧使用 lean 请求，跳过 App 状态双向序列化；事件与服务帧继续活跃会话并输出完整 checkpoint，失败后从宿主的上一份好状态恢复。重载前只读取得最新 checkpoint，不丢弃逻辑动画更新。普通 TextInput 通过逻辑路径同步文本、placeholder 与绝对提交计数；`#[serde(skip)]` 缓冲在普通事件中保持，兼容重载时由宿主恢复内容且不重复提交。密码输入在热重载预览中显式禁用，正式 native 路径不受影响。未消费的路径事件计入 `Frame.unmatched_events`；计数能发现丢弃，但不能识别错误匹配到另一个合法控件的情况。协议、恢复边界和验证脚本见 [热重载协议 v2](hotreload-protocol.md)。

```sh
plico dev apps/gallery
plico dev apps/gallery --no-watch
plico dev apps/gallery -- --width 920 --height 780
plico dev /path/to/app --framework /path/to/plico
```

框架来源默认是 CLI 构建时的 Plico checkout，可通过全局 --framework 指定。CLI 源码路径失效时需显式指定。独立 app 的 Cargo.toml 不加入框架 workspace，后续 app 开发无需修改框架 manifest。

## 编译成本

开发默认共用 Plico checkout 的 target；CARGO_TARGET_DIR 可覆盖。host/app 使用同一个 target、dev opt-level=1、incremental=true；不再强制 UI 模块 opt-level=0，应用自身的 package profile 仍可覆盖。生成模板将依赖 opt-level 设为 2。不要为每一版 app 建立新 target，不添加变化的 RUSTFLAGS，不在 reload 前 cargo clean。

应用改动只使其 crate 失效，未改的依赖由 Cargo 复用。首次构建、工具链/feature/依赖/profile/RUSTFLAGS 改动仍可能重编译；不是“任何条件下永不编译依赖”的承诺。host 源码或协议改变后重新启动 dev，app 热更新不替代框架自身的重建。

`target/plico-dev/<package>/build-report.json` 记录 Cargo artifact 的 fresh 标记和 build 用时。可区分真正重编译、无修改的缓存命中与最后链接；不把单次时间当作通用性能基准。

## 其他命令

| 命令 | 行为 |
|---|---|
| new PATH [--name NAME] | 生成使用 component 自动身份的独立应用（Plico、plico-dev、Serde、Stabby） |
| render --out PATH | 静态 gallery 的真实后端离屏 PNG |
| probe | 全部 gallery 页面的后端工作量 JSON（cold/idle/scroll/desktop），含单次粗计时；--legacy 为原 Controls 交互脚本 |
| check | cargo check --workspace --all-targets |
| test | cargo test --workspace |
| fmt | rustfmt + Plico 组件 formatter；加 --check 只检查 |
| clippy | cargo clippy --workspace --all-targets |

```sh
plico render --out /tmp/gallery.png --width 920 --height 780 --dark --large
plico test --all-features
plico clippy -- -D warnings
```

check/test/clippy 等 Cargo 包装命令的后续参数原样转交（保留 --）；fmt 使用下面的独立选项。参数通过 Command 传递，不拼 shell 字符串。相对输出路径相对执行目录；-C 改变该目录。从 workspace 子目录执行也可找到正确 manifest。

## 格式化

```sh
plico fmt                             # 格式化整个 workspace
plico fmt --check                     # 只检查，存在差异时返回 1
plico fmt -p gallery            # 指定一个 workspace package
plico fmt src/lib.rs                  # 指定文件或目录
plico fmt --width 100 src             # 组件链的平铺宽度，默认 120
```

`plico fmt` 是原生 Rust 实现，不需要 Python 或 uv；仍需要工具链提供 rustfmt。
Tree-sitter 依赖只进入 CLI，不进入 Plico 应用。普通 Rust 使用文件所属 package 的
edition，以及从文件所在目录查找的 rustfmt 配置。组件平铺宽度独立于 rustfmt 的 max_width。

同时带 `#[component]`（或 `#[plico::component]`）与 `#[rustfmt::skip]` 的函数由
Plico 接管：借 rustfmt 处理内部 Rust，再按宽度将方法链逐段装行（续行也继续平铺），
短布局保留 `+ |cx| {`；布局头跨行时将 `{` 独立换行，并使正文只增加一级缩进。
简单的 `if ... { 值 } else { 值 }` 在整行能容纳时收为一行；含注释、多条语句或多行字符串的分支不压缩。其他代码仍交给 rustfmt。
原生 children 同样支持。其他 skip 函数保持跳过；不需要配置 format-on-save。
旧双分号布局已经移除，formatter 会明确报迁移错误，不再维护空语句掩码和恢复流程。

默认扫描 workspace 成员的 src/tests/examples/benches、build.rs 和显式 Cargo target 入口。
也可直接给出文件或目录；这时不与 --package 混用。目录扫描跳过软链接、target、.git、
node_modules 和 reports。所有文件完成解析与格式化后才开始写回，写回使用同目录临时文件
原子替换并保留文件权限。语法错误会报告文件与位置；--check 不改写源码。

目前不支持嵌套的、同时带 component 和 skip 的函数，也不解析任意属性别名。
链平铺规则应用于受控函数内的方法链，不只限于 UI 构造函数；宽度按 Unicode 字符计数，
不是终端显示列数。单个无法再拆的方法片段、长字符串可能超过宽度；参数结构交给 rustfmt。
格式化、编译和编辑器集成互相独立。

## 自动验证与边界

```sh
cargo build -p plico-cli
python3 scripts/check-hotreload.py
```

测试创建临时独立 app，使用真实 watcher、编译器、动态库和常驻离屏宿主验证：同一 PID、兼容状态保留、schema 变化重置、编译失败、panic/ABI 错误拒绝、恢复后继续交互、依赖 fresh、文字/空行编辑后的自动 ID 一致，以及真实 Vello PNG。不打开用户桌面窗口、不修改用户应用或框架源码。

`dev --headless` 是可脚本驱动的 resident host，stdin JSON 命令支持 snapshot、click(role/label)、capture(out)，每行响应一份 JSON；供回归测试使用。

当前旧代码映像保持映射至进程结束，避免 TLS/析构地址悬空，长时间大量重载会增加地址空间/内存占用；可重启 dev 回收。Stabby 简化并校验 ABI，不提供进程隔离，不能恢复原生崩溃、abort、OOM 或任意 unsafe 错误。只加载可信的本地应用代码。宿主输入、文字编辑和异步任务等后续能力仍需分别完善。

内置 gallery 同样是普通应用：`cargo run -p gallery` 打开窗口，`cargo plico render --out /tmp/gallery.png` 离屏绘制；`cargo plico render/probe` 调用 gallery 的独立验证示例。后端仅在测试中依赖 gallery，生产依赖方向为 gallery → backend。`cargo build -p gallery --lib --no-default-features --features abi` 构建轻量热重载模块。

## Gallery 验证工具

`gallery/src/main.rs` 只构造 App 和窗口选项，不包含参数解析。截图与工作量采样放在 examples/capture.rs、examples/probe.rs，由 CLI 调用；验证工具的参数继续使用 usage-rs。

```sh
cargo plico render --out /tmp/layout.png --page layout
cargo plico render --out /tmp/controls.png --page controls --dark
cargo plico render --out /tmp/motion.png --page motion --expanded
```

--width/--height 仅用于截图验证，正常窗口初始大小在 main.rs 中配置。截图示例不进入普通应用的启动路径。

## 应用自己的常驻宿主

`[package.metadata.plico] host = "binary-name"` 让 dev 构建同一个 package 的 main.rs binary 并保持进程，随后只替换 lib.rs 模块。使用 `run_service_app_with_options` 在直接调用与动态模块间选择；宿主代码变化需重启。示例：`cargo plico dev apps/services-demo`。自定义宿主暂不提供通用 `--headless` JSON stdin 接口，完整桥接和验证见 [宿主服务](host-services.md)。

Headless 验证通路另外接受 `{"action":"text","text":"hello"}` 和 `{"action":"key","key":"Enter"}`，发送给当前聚焦控件。先使用 `focus` 选择语义角色 `textbox` 和对应 label；这些操作经过原生 adapter，不直接修改应用状态。
