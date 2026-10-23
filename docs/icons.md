# 应用按需生成 Lucide 图标

Plico 的核心只内置控件需要的 CHECK、CHEVRON_DOWN、CHEVRON_RIGHT、X。
应用通过 CLI 选择自己的图标，生成独立 Rust 常量；不再编译完整图标集合。
当前 Lucide 来源固定为提交 `02eb9161b57917ccaa262f1bf070507cddde3b08`，
目录中的 1,899 个 canonical 图标均可选择，品牌 logo 仍由应用自行维护。

## 使用

在应用目录执行（不要求当前目录是 Cargo workspace）：

```sh
plico icons add lucide:save lucide:search lucide:settings

# 主线 immediate 应用直接依赖 plico-core 时：
plico icons add lucide:save --rust-crate plico_core

# 同一个输出再次 add 会合并已有集合，重复添加不会改变输出。
plico icons add lucide:folder

# 离线验证，不访问网络、不调用 Cargo、不要求下载缓存。
plico icons check
```

默认生成并提交以下文件：

- `src/icons.rs`：每个图标一个 SvgAsset 常量，直接内嵌上游完整 SVG 字节。
- `src/icons.rs.json`：来源版本、所选图标及内容哈希、Rust 文件哈希。
- `src/icons.rs.LICENSE`：上游完整许可证。

首次生成会下载所选图标和许可证；新增图标时保留已有常量，从生成结果复用已有 SVG。
普通 Cargo 构建只使用生成 Rust，不要求 CLI、Bun、网络或 SVG 文件。
SHA-256 校验会拒绝与固定来源不符的下载内容。生成器拒绝覆盖手写文件或被手改的输出，
且会在下载和写文件前验证全部输入名称。不要手改生成内容，应再次使用 add。
不同上游 pin 的集合不会被静默合并。

在 crate 根添加模块，然后正常使用：

```rust
mod icons;

// 组件内部：
icon(cx, &icons::SEARCH).color(Color::Muted);
button(cx).icon(&icons::SAVE).text("Save").primary();
```

`--output path/to/icons.rs` 可以生成多个独立模块；`--rust-crate NAME` 指定提供
SvgAsset 的 Rust 依赖名，默认 plico_core，支持依赖重命名。仅在生成核心内部模块时使用 crate。
输出名为 kebab-case 图标名的 SCREAMING_SNAKE_CASE；数字开头加 ICON_。
不提供全量 registry、名称运行时查找或预加载，也不通过扫描 Rust 推测使用集合。

## 离线与维护

有本地下载缓存时可以直接生成，不需要网络：

```sh
plico icons add lucide:save --source /path/to/lucide
```

缓存根包含 `icons/save.svg` 和 `LICENSE`，字节必须匹配 CLI 内嵌的来源目录哈希。
框架的目录元数据位于 crates/plico-core/assets/lucide/SOURCE.json；原始 SVG 只作本地
缓存，不提交。应用生成的 Rust、选择清单和许可证才是构建输入。

`scripts/icons.ts --fetch SHA` 是框架维护者的上游获取工具，会更新目录元数据和缓存；
随后委托 Rust CLI 生成核心的四个控件图标。`--check` 委托 CLI 校验该集合。
更换上游 pin 是显式迁移，旧选择清单不会被新 CLI 静默升级。旧 Python 入口只转发到该脚本。

## 验证边界

CLI 测试覆盖离线生成、增量合并、无缓存校验、未知名称、路径输入、错误哈希、
手写输出保护和许可证/清单篡改。后端测试解析核心和测试用到的图标，普通测试不再遍历
上游全部图标。Gallery、Launcher、services demo、Terminal 和 Web UI
各自提交使用到的集合。按需链接仍受具体链接选项影响，不承诺所有配置的最终体积。

2026-10-23 验证：CLI、core、后端 lib/tests 共 277 项通过，workspace all-targets
编译与 CLI 严格 Clippy 通过；移走 SVG 缓存后，workspace 离线编译、维护脚本和全部
生成集合的离线校验仍通过。真实网络路径已验证单独下载 SAVE 并生成可校验的集合。

## 外观与生命周期

icon 返回 Svg builder，是 Context 首参函数；component 为它生成静态身份。默认大小跟随继承字号，.size 可覆盖。Lucide 的 currentColor 继承当前文字颜色，.color 可覆盖；明确的 SVG fill/stroke 色保留，多色 logo 不受影响。SVG 子内容为装饰，不增加 Tab 停靠点，父按钮保留名称与点击归属。

SVG 按源文与解析颜色共享 Arc；固定色资源只按源文共享。主题或新颜色引入时清理不再使用的颜色版本，最后引用卸载时释放。复选框使用 Lucide CHECK SVG，on_accent 随主题，淡入/缩放不触发布局或逐帧重新解析。普通无变化帧继续复用场景。

全量可选资产与全量打包是两件事。按需链接已经有最小样本证据；热重载协议中重复 SVG XML 的传输去重仍未完成。

## Generated SVG markup

For runtime data graphics use `svg_source(cx, markup: String)` instead of an
embedded `SvgAsset`. It has the same size/color/motion builder and supports
`.width_percent(100.)`. Cache generated markup when data changes; source strings
are still owned by declarations, while the renderer retains unchanged SVGs.
This does not embed a WebView or require leaking strings as static assets.
