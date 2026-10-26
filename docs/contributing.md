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

普通 pull request / main push 保留两个 CI job：Linux **Lightweight Rust checks** 与 **Windows compile**。
Windows 等 Linux 检查成功后再执行。纯文档与发布配置改动通过 GitHub 原生
`paths-ignore` 跳过整个 CI；手动运行仍执行全部检查。不维护自定义变更分类器，
也不为筛选逻辑调用 GitHub API。Linux 负责：

- 检查 Rust 与 component 格式和 Widget 边界；所有 Cargo 命令使用 `--locked`。
- 测试 `plico-core`、`plico-macros`、`plico-motion`、`plico-dev`、
  `plico-cli`、`plico-markdown`，以及无窗口的 runtime 语义协议测试。
- 其余主线代码通过 workspace Clippy 检查（默认 feature）；不启用 Skia SDK、
  FLTK、LVGL、ESP-IDF 等可选后端。`backend-settings` 使用 Vello feature 参与编译；全局热键启用 `plico-shell/global-hotkeys` 做编译检查，不运行 OS 自动化。
- formatter、测试与 Clippy 在同一 runner 复用缓存；取消同分支的过期运行，15 分钟超时。

Windows 只运行同一主线默认 feature 的 `cargo check --all-targets`，不运行测试或桌面自动化，20 分钟超时。

CI 不启动窗口/GPU、不截图，不安装 mise 工具集、不运行 release-plz 仿真或
cargo-machete，不设置全量/窗口验收开关，也不增加定时全量任务。
headless 热重载进程验收、其他平台和可选后端验证脚本保留为本地工具。

这是成本优先的基本门槛，不等于所有后端都通过验证。发布前按改动范围在本地检查
可选后端、热重载、IME、窗口和性能。main 的 Git tag / GitHub Release 仍由独立
Release workflow 管理。Actions 固定到 commit，不依赖本机 mbx 或原 Wabou checkout。

## 统一版本

workspace 成员（框架、后端、CLI 和示例应用）统一使用根 `Cargo.toml` 的
`[workspace.package].version`，当前为 `0.1.0`。成员声明 `version.workspace = true`；
release-plz 负责根版本、changelog 和 Cargo.lock 更新，不另写版本同步脚本。
成员使用 Cargo 原生的版本继承；CI 的 `--locked` 构建负责拒绝过期锁文件。
旧 release PR 若早于 workspace 结构变更，应重新生成后再合并。

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
只由 `plico-cli` 创建 tag 和 GitHub Release；标题显式使用 `Plico v{{ version }}`，
避免默认显示 CLI 包名。release PR 的 `pr_name` / `pr_body` 也显式使用 Plico，
不使用默认的逐 package changelog 标题；`plico-cli` 只是内部发布锚点，不代表只发布 CLI。
其他 package 的变更汇总进根 `CHANGELOG.md`。
第一次 release PR 保持当前声明的版本，之后按 Conventional Commits 更新版本。
`changelog_include` 只列框架 crate。`apps/` 是测试/展示应用，根目录 `examples`
也是非发布辅助包；它们配置 `release = false` 与 `git_only = false`，不独立计算
下个版本，也不纳入框架 changelog。它们仍继承 workspace 版本；框架升级时一同更新。
新增应用时同步补齐这两项配置，不把普通 demo 改动作为框架发版依据。

release-plz 0.4.0 的 Git 基线重建要求旧 tag 中存在当前分析的包；`examples`
不存在于 `v0.1.1` 中。跳过这些辅助包也避免了此问题，不需要改写已发布 tag。
新增框架 crate 时仍需检查其发布基线。

仓库 Settings → Actions → General 必须允许 Actions 创建 PR。
workflow 默认使用 `GITHUB_TOKEN`；它创建的 release PR 不会自动触发后续 CI。
如果需要 release PR 和 tag 自动触发其他 workflow，配置仓库 secret
`RELEASE_PLZ_TOKEN`，使用仅对本仓库拥有 Contents 与 Pull requests 读写权限的
fine-grained PAT，workflow 会优先使用它。没有这个 secret 时，可由维护者手动
关闭并重新打开 release PR 以触发 CI。这里不需要设置任何 Cargo registry secret。

需要预览版本更新时，在独立 checkout 中直接运行官方命令：

```sh
mise exec -- release-plz update
cargo check --locked -p plico-cli
git diff -- Cargo.toml Cargo.lock CHANGELOG.md
```

`release-plz update` 会修改该 checkout 的版本与 changelog，但不会创建远端 PR、
推送 tag 或发布 crate。不另维护一套发布模拟器。

## 确定性的应用测试

优先使用 [Rust 测试协议](testing.md)：`plico_runtime::testing::Driver` 驱动真实声明、布局和输入路径，
以角色和名称定位控件，用虚拟时钟推进动画，不依赖桌面坐标、系统焦点或 sleep。
开发宿主的 headless 命令复用同一个 dispatcher，不另写一套事件逻辑。

普通 CI 不运行桌面自动化。这些 Python 窗口脚本只保留为本地验收工具，
不提供额外的全量 CI 开关。
