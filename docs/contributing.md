# 提交规范

使用 Cocogitto 校验 Conventional Commits。工具由根目录 mise.toml 声明；新 checkout 后执行：

```sh
mise install
mise exec -- cog install-hook commit-msg
```

Git hook 是本地配置，不会随 clone 自动安装。commit-msg hook 仅校验当前提交信息；完整历史使用 cog check 验证，不在消息 hook 中重复运行完整构建。

## 提交格式

```text
<type>(<scope>): <description>
```

scope 可选。常用 type 为 feat、fix、refactor、perf、test、docs、build、ci、chore、style、revert；例如：

```text
feat(ui): add controlled text input
fix(runtime): preserve focus when keyed children move
docs: explain animation lifecycle
```

可选 scope 按实际改动使用 core、ui、runtime、backend、gallery、tooling 等，不强制固定清单。存在破坏性变更时使用 Conventional Commits 的 ! 或 BREAKING CHANGE footer，Cocogitto commit 可用 --breaking-change。

```sh
git add src tests
cog commit feat "add controlled text input" ui
# 普通 git commit 也会经过已安装的 commit-msg hook
cog check
```

按独立行为组织提交，描述改动结果。只运行与改动相关的检查；代码改动可根据范围运行 cargo test、cargo clippy 与格式检查。当前不配置自动发布、版本 bump 或打 tag。

## CI

GitHub Actions 的 `.github/workflows/ci.yml` 在 push、pull request 和手动触发时运行，
使用 stable Rust、Cargo.lock 和只读仓库权限；同一分支的新运行取消旧运行。Action 固定
到 commit，Cargo 依赖按任务缓存，不依赖本机 mbx、原 Wabou checkout 或 Lucide SVG 缓存。

- **Formatting and workspace versions**：检查成员版本统一继承，使用 `plico fmt --check`
  同时检查普通 Rust 与 `#[component]` 区域，不对 UI 强行套用 rustfmt 的布局。
- **Linux tests, Clippy and hot reload**：安装字体、原生开发库和 Mesa 软件 Vulkan，
  对全部 workspace target/feature 执行 Clippy（警告视为错误）和测试，包括离屏 GPU
  绘制和 profiling 双输出模式；运行实际 CLI 的生成应用、热重载失败恢复和原生文本输入脚本。
- **WASM host and UI**：检查无渲染核心、gallery/split 浏览器宿主及可加载 UI 模块，
  宿主路径同时执行 Clippy。只检查对应浏览器 package，不把原生 workspace 强编成 WASM。
- **LVGL backend experiment**：使用 stable 和 SDL2/libclang，离屏执行
  [LVGL 最小实验](lvgl.md)；普通全 feature 任务排除 `plico-lvgl` 和 P4 实验，LVGL 任务单独验证 SDL2 输入、关闭与截图。

这套 CI 不等于跨平台窗口、浏览器交互或性能验收。macOS/Windows 原生窗口、完整 IME、
浏览器 smoke 和 CPU/GPU 性能仍需对应环境验证；不把受负载影响的微秒耗时设为硬门槛。
离屏软件 Vulkan 也不代表真实显卡性能。脚本生成的 reports 留在 CI 工作目录，不提交。

本地对应命令：

```sh
python3 scripts/check-versions.py
cargo run --locked -p plico-cli -- fmt --check
cargo clippy --locked --workspace --exclude plico-lvgl --exclude plico-lvgl-esp32p4 --all-targets --all-features -- -D warnings
cargo test --locked --workspace --exclude plico-lvgl --exclude plico-lvgl-esp32p4 --all-features
cargo build --locked -p plico-cli -p plico-vello
python3 scripts/check-hotreload.py
python3 scripts/check-text-input-reload.py
```

WASM 还需要 `rustup target add wasm32-unknown-unknown`，具体检查命令见 workflow。
热重载脚本默认从 `target/debug` 找 CLI；自定义输出目录时设置绝对路径的 `CARGO_TARGET_DIR`。

## 统一版本

workspace 成员（框架、后端、CLI 和示例应用）统一使用根 `Cargo.toml` 的
`[workspace.package].version`，当前为 `0.1.0`。成员声明 `version.workspace = true`；
升级时只改根版本并更新 Cargo.lock，`scripts/check-versions.py` 会拒绝单独写版本的成员。

排除在 workspace 之外的 `crates/plico-vello/reference` 保留历史版本；`plico new` 创建的
独立用户应用有自己的版本，不随框架一起修改。包版本不代替热重载协议或 ABI 的兼容性检查。
本次统一版本没有发布 crate，也没有修改协议版本或增加自动发布流程。
