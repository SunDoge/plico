# TypeScript recipe 开发循环

当前已实现：TypeScript 组合 fixture 和交互 → 生成版本化 JSON 计划 → Rust 批量执行。
不需要 C#、嵌入式 JS 引擎、动态库或应用状态反射。普通运行的 App 不要求序列化。
已有 `PreviewScenario` JSON 和 `plico capture` 保持原来的接口；测试计划是独立协议。

## 独立应用示例

应用级 Warden 套件已迁到独立的 **Plicowarden** 仓库，见 [仓库边界](plicowarden.md)。
以下应用命令和示例路径以 `../plicowarden` 为工作目录；Plico 中只保留通用 TS helper、Rust 协议与框架测试。

## 运行

从 Plicowarden 仓库根目录执行，需要 Bun 1.4.4。Bun 管理锁定依赖并直接执行 TS，另行运行
TypeScript 编译器检查类型；运行 TS 不等于已经检查类型。

```sh
# 首次准备 TS 依赖、检查类型、生成计划、构建一次 Rust runner、执行。
python3 scripts/run-recipes.py

# 只改 TS / fixture / 交互后，复用已编译 Rust，不调用 Cargo。
python3 scripts/run-recipes.py --no-build

# 快速交互与语义检查，不初始化 GPU，不输出 PNG。
python3 scripts/run-recipes.py --no-build --semantic

# 自选套件和输出。
python3 scripts/run-recipes.py --no-build \
  --suite recipes/suite.ts --output reports/my-recipes
```

脚本通过 `bun install --frozen-lockfile` 检查并安装缺失的 TS 工具依赖。Bun 仅是测试编写工具，
不是 Plico 桌面运行前提。当前脚本构建的是 Warden example，不是任意应用的通用 CLI；
换应用需要提供自己的 typed factory 和事件处理入口。
`--no-build` 明确要求调用者保证 Rust executable 与源码相符；源码变化后去掉该参数。
`--runner` 可指定预编译 executable；正常构建从 Cargo artifact 消息取得实际路径。

也可分别使用已有产物：

```sh
bun install --cwd tools/recipes --frozen-lockfile
bun run --cwd tools/recipes check
bun tools/recipes/src/generate.ts recipes/suite.ts reports/recipes/plan.json
cargo build --locked -p plicowarden --example recipes
target/debug/examples/recipes --plan reports/recipes/plan.json --output reports/recipes/run
```

## 公共状态与流程

Warden 的实际套件在 `recipes/suite.ts`，公共状态在 `fixtures/vault.ts`，
公共搜索流程在 `flows/search.ts`。每次 fixture 调用都返回新对象，套件展开时再次复制，
运行时每例创建全新 App、Source、输入上下文和 VirtualClock；共享 renderer 与字体/GPU 缓存。

```ts
import { button, capture, expect, matrix, recipe, suite } from "../tools/recipes/src/index.ts";
import { demoVault, viewports } from "./fixtures/vault.ts";
import { search } from "./flows/search.ts";

export default suite(matrix(viewports, viewport => recipe(`search-${viewport.name}`, {
  state: demoVault(),
  viewport,
  steps: [
    ...search("github"),
    expect(button("Select GitHub")).exists(),
    expect(button("Select Linear")).absent(),
    capture("filtered"),
    ...search(""),
    capture("cleared"),
  ],
})));
```

TS 中的 import、函数、循环、条件和 matrix 都在生成计划时执行。Rust 接收的是展开后的
固定步骤，不执行 TS，也不运行依赖 UI 返回值的任意脚本。`flow(name, steps)` 为步骤附加
公共流程名称，保留其源位置；不会为每次截图重新执行前面的操作。
fixture、主题和窗口参数是显式测试输入，不是任意写入活跃应用字段的接口。

## 操作和断言

- `click` / `focus`：精确 role + accessible label，重复目标显式指定 occurrence。
- `fill(label, text)`：原生焦点、平台 Select All、Backspace、文本输入，既支持替换也支持清空。
- `typeText`、`key`、`setValue`：对应现有 runtime 命令。
- `advance(ms)`：推进虚拟时钟，不睡眠、不自动模拟中间 vsync。
- `waitFor(expect(...), { timeoutMs, intervalMs })`：立即检查断言；未满足时按
  `intervalMs`（默认 16 ms）推进虚拟时钟并重建，直到成功或精确到达虚拟超时。
  它不读取墙钟，也不会用 `sleep`，适合等待动画或由确定性 fixture 驱动的 UI 状态。
- `expect(selector)`：exists、absent、count、enabled、value、selected。
- `expectText(text)`：检查最终声明/布局中的精确文字，不要求该文字有交互语义标签。
- `capture(name)`：捕获当前流程状态，允许一例多个命名截图。
- `event(value)`：交给应用自己的 synthetic host/service 处理函数。

`exists` 检查暴露的语义树，不等于元素在滚动裁切区内可见。
`enabled` / `value` / `selected` 要求唯一匹配；count 可配合 role + label_prefix。
普通输入值现在进入语义快照；秘密 TextBuffer 的内容仍不进入该 value，失败证据也不读取秘密值。
语义模式使用 renderer-independent 文字服务，只验证交互与声明，不能代替真实 renderer 的文字度量或截图。

## 服务结果注入

Warden example 不构造真实 `Host`，不打开 SDK worker，不读取用户偏好。
`fixtures/events.ts` 的 `accountError(message)` 要求存在待处理账户请求，消费其 ticket 后，
通过 `Warden::apply(Event)` 回到现有 UI 处理路径。`lockVault()` 注入锁定通知。
事件 JSON 由应用定义的 Rust enum 严格反序列化；未知事件不能变成任意字段修改。

当前已验证「填写合成凭据 → 登录按钮禁用 → 推进虚拟时间 → 注入服务失败 → 检查错误 → 回到 demo」。
这不是网络或 SDK 集成测试，也不是完整的请求自动匹配/异步调度系统。
应用可以在事件 hook 中接入更多 typed fixture 消息，不能让宿主服务持有 UI 状态引用。

## 计划和报告

`RecipePlan<S>` 的 version 当前为 1。case 包含 name、应用定义的 state、viewport、steps。
每步包含 source（TS 文件/行/列）、flow 路径和 operation。框架严格解析未知字段，
检查版本、视口和重复名称；案例和截图名仅允许 ASCII 字母数字、下划线和连字符。
`_failure` 为保留截图名。预览 JSON 与测试计划虽都为 version 1，但入口和结构不同，不自动混用。

输出目录中：

- `input-plan.json` / `plan.json`：展开后的输入，直接用于重放，不需要 TS 源码或 Bun。
- `results.json`：逐例/逐步结果、源位置、虚拟时间、耗时和截图路径。
- `<case>/<capture>.png`：多个检查点的图像。
- 失败时的 `<case>/_failure.png`、最后一个通过的截图路径、语义节点和 rendered_text。
- `pipeline.json`：类型检查、生成、构建（如果执行）、运行的独立墙钟耗时。

一步失败则停止当前案例，后续独立案例继续；任何案例失败使进程返回非零状态。
错误源位置来自 Bun 执行 TS 时的调用栈，生成后规范为仓库相对路径，
公共 helper 中的错误指向 helper，并同时显示 flow 名称。它不是任意 transpiler 的 source map 支持。
脚本有执行超时；单进程 recipe 本身不隔离 panic、GPU device loss 或无限循环。

输出只用于合成 fixture，勿放真实凭据；计划会保留输入数据用于重放。
生成产物保存在忽略的 reports/ 下，不参与提交。

## 验证

```sh
# 在 Plico 目录运行框架检查
bun install --cwd tools/recipes --frozen-lockfile
bun run --cwd tools/recipes check
bun run --cwd tools/recipes test
cargo test --locked -p plico-desktop --no-default-features --features wgpu --test recipes
cargo test --locked -p plico-desktop --no-default-features --features wgpu \
  --test recipes --test preview_session --test capture_wgpu -- --ignored
cargo test --locked -p plico-runtime --test focus_chrome --test regions_controls
# 在独立 Plicowarden 目录运行应用端到端检查
cd ../plicowarden
cargo build --locked -p plicowarden --example recipes
python3 tools/recipes/test/e2e.py
```

端到端测试实际执行 TS import → JSON → Rust，验证失败步骤源位置、flow 名称、现场截图、
后续案例继续、未知事件拒绝以及非法计划在 factory 执行前被拒绝。
GPU 测试验证连续交互的多个截图不同、失败截图保存当前状态、下个案例恢复初始状态。

## 本机反馈耗时

2026-11-08 的下表是迁移到 Bun 前，Linux / wgpu / Node 26.12.1、复用已构建 runner 的
五轮历史中位数；不能直接当作当前 Bun 路径的耗时：

| 模式 | 类型检查 | TS→JSON | Rust 执行 | Python 命令总耗时 |
|---|---:|---:|---:|---:|
| 仅语义 | 0.197 s | 0.091 s | 0.280 s | 0.595 s |
| 断言 + 截图 | 0.194 s | 0.091 s | 1.256 s | 1.579 s |

同一套七例，视觉模式每轮 14 个截图点；五轮 PNG 的 SHA-256 全部一致。
视觉模式的步骤断言和截图在同一条交互中执行，不再像早期 probe 那样分别重放。
这是热环境、固定合成数据和固定字体的本机结果；不含 Rust 构建，不是首次依赖安装时间。

## 后续边界

尚未实现按源码依赖选择用例、图片 baseline 管理、常驻 IPC、自动等待真实服务、
可序列化的运行中 App 快照、生成 UI 的任意随机源控制，或 TS 读取 UI 后实时分支。
目前 fixture + 输入 + 虚拟时间覆盖的固定流程可重放；用到随机生成器时仍需应用单独注入确定性随机源。
保持这条主线，先从真实案例提炼新操作，避免预先扩展成完整脚本运行时。
