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

按独立行为组织提交，描述改动结果。只运行与改动相关的检查；代码改动可根据范围运行 cargo test、cargo clippy 与格式检查。版本更新和 Git tag 由下述 release-plz 流程管理；暂不发布到 crates.io。

## CI

GitHub Actions 的 `.github/workflows/ci.yml` 在 main 分支 push、pull request 和手动触发时运行，
使用 stable Rust、Cargo.lock 和只读仓库权限；同一分支的新运行取消旧运行。Action 固定
到 commit，Cargo 依赖按任务缓存，不依赖本机 mbx、原 Wabou checkout 或 Lucide SVG 缓存。
质量检查每轮都执行；仅文档、Dependabot 配置、release-plz 配置等已知非构建输入变化时，
跳过原生和 WASM 构建。`scripts/ci-changes.py` 对其他路径一律保留完整检查，历史读取失败
也回退到完整检查；main 还会对照上次成功 CI 的提交，避免漏测被取消运行中的源码变化。
手动运行始终全量验证。PR 只恢复 Cargo 缓存，由 main 更新缓存。
Skia 在同一个 window feature 图中同时执行普通与 ignored Vulkan 测试，避免切换 feature 后
重复编译；窗口例子不启用无关的默认热重载 feature。

- **Formatting and workspace versions**：检查成员版本统一继承，使用 `plico fmt --check`
  同时检查普通 Rust 与 `#[component]` 区域，不对 UI 强行套用 rustfmt 的布局。
- **Linux tests**：core、rendering、apps 三组并行测试，分组在 `scripts/ci-tests.py`
  中维护并检查 workspace 覆盖；rendering 同时验证 profiling 双输出模式。
- **Native hot reload and text input**：独立编译 CLI/宿主，运行生成应用、热重载失败恢复
  和原生文本输入脚本，不等待整套 workspace 测试。
- **Skia Vulkan and native windows**：独立检查 Skia 与共享 Settings 应用，验证离屏回放、
  Vulkan 生命周期，以及开启 validation layer 的 Xvfb 窗口交互；失败时上传窗口截图和日志。
- **Linux Clippy**：检查普通 workspace 的全部 target/feature，警告视为错误；
  FLTK、LVGL 和共享 Settings 由对应后端任务检查。
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
python3 scripts/ci-tests.py --check
python3 scripts/ci-tests.py core
python3 scripts/ci-tests.py rendering
python3 scripts/ci-tests.py apps
cargo test --locked -p plico-skia -p plico-backend-settings --features plico-skia/window,plico-backend-settings/skia --all-targets
cargo build --locked -p plico-cli -p plico-vello
python3 scripts/check-hotreload.py
python3 scripts/check-text-input-reload.py
```

WASM 还需要 `rustup target add wasm32-unknown-unknown`，具体检查命令见 workflow。
三组测试、热重载与 Skia 任务使用独立 Cargo 缓存；新增 workspace 成员必须明确分配测试组。
热重载脚本默认从 `target/debug` 找 CLI；自定义输出目录时设置绝对路径的 `CARGO_TARGET_DIR`。

## 统一版本

workspace 成员（框架、后端、CLI 和示例应用）统一使用根 `Cargo.toml` 的
`[workspace.package].version`，当前为 `0.1.0`。成员声明 `version.workspace = true`；
升级时只改根版本并更新 Cargo.lock，`scripts/check-versions.py` 会拒绝单独写版本的成员。

排除在 workspace 之外的 `crates/plico-vello/reference` 保留历史版本；`plico new` 创建的
独立用户应用有自己的版本，不随框架一起修改。包版本不代替热重载协议或 ABI 的兼容性检查。
统一包版本不改变协议版本；目前通过 release-plz 管理 Git-only 发布。

## Git-only release

`.github/workflows/release.yml` 使用固定版本的 release-plz。main 更新时先执行
`release`，再执行 `release-pr`：普通提交只创建或更新 release PR；合并
`release-plz-*` 分支的 PR 后，才创建统一的 `vX.Y.Z` tag 和 GitHub Release，
Release 说明来自根 changelog。也可在 main 手动运行
Release workflow。合并 release PR 前应确认 CI 通过，workflow 本身不等待 CI。

`release-plz.toml` 使用 `git_only = true`，以 Git tag 为版本基线，不从 Cargo registry 查询已发布版本，
也不发布 crate；不需要 crates.io token，也没有 trusted publishing 权限。
目前不上传安装包。所有成员继续继承 workspace 版本，
只由 `plico-cli` 创建 tag 和 GitHub Release，其他 package 的变更汇总进根 `CHANGELOG.md`。
第一次 release PR 保持当前声明的版本，之后按 Conventional Commits 更新版本。
新增 workspace 成员时，将它加入 `changelog_include`。

仓库 Settings → Actions → General 必须允许 Actions 创建 PR。
workflow 默认使用 `GITHUB_TOKEN`；它创建的 release PR 不会自动触发后续 CI。
如果需要 release PR 和 tag 自动触发其他 workflow，配置仓库 secret
`RELEASE_PLZ_TOKEN`，使用仅对本仓库拥有 Contents 与 Pull requests 读写权限的
fine-grained PAT，workflow 会优先使用它。没有这个 secret 时，可由维护者手动
关闭并重新打开 release PR 以触发 CI。这里不需要设置任何 Cargo registry secret。

本地验证不会创建远端 PR、推送 tag 或发布 crate：

```sh
mise install
mise exec -- python3 scripts/check-release.py
```

该脚本在临时仓库中验证首次版本、仅改独立后端时的统一版本升级、changelog 汇总和
无变更重跑稳定性。它不会更改当前工作区、版本或 tag。
