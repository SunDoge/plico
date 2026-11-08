# Recipe 批处理实验

> Warden 应用与应用 benchmark 已迁到独立 Plicowarden 仓库；下文为迁移前的实验记录。重跑应用脚本需在 `../plicowarden` 中使用新包名 `plicowarden`。见 [仓库边界](../plicowarden.md)。

2026-11-08，本机 Linux / wgpu，使用 Rust Warden 的七个合成场景：vault、
settings、login、search、compact、editor、generator。所有场景不读取账户、网络或用户偏好。
本实验比较运行策略，不比较语言，也不包含首次全量编译。

## 可重建方式

```sh
python3 scripts/bench-recipes.py
```

脚本在开头调用一次 `cargo build --locked --offline -p warden --example recipe_probe`，
随后直接调用生成的 executable。各策略先预热一次，再以固定随机顺序测五轮。
构建命令单独记录，不计入下面的 recipe 执行时间；这次构建可能是 no-op，不能视作冷构建数据。
每轮使用不同输出目录，保留 JSON 结果、逐案例阶段计时和 PNG。

复用编译产物单独运行某套 recipe：

```sh
cargo build --locked -p warden --example recipe_probe
# 以下命令可重复运行；修改 JSON 后无需重新编译。
target/debug/examples/recipe_probe \
  --manifest apps/warden/scenarios/recipes.json \
  --mode shared --output reports/recipes
```

目前 manifest 的 scenario 路径相对工作目录，上述命令从仓库根目录执行。
manifest 是实验 runner 的输入，不是新增的框架标准或 `plico capture` CLI 参数。

## 比较方法

- 独立进程串行：七次启动，每次一个 recipe，每次创建 renderer。
- 独立进程并行：最多两个进程，其他工作相同。
- 单进程、独立 renderer：一次启动，逐个创建 renderer。
- 单进程、共享 renderer：一次启动，复用 renderer / 字体和 GPU 缓存。
- 单进程、仅语义断言：不用 GPU、不写 PNG，执行相同输入和语义断言。

每个 recipe 都创建全新的 App、Source、输入状态、TextContext 和 VirtualClock。
共享的是后端资源，不是从上一个 recipe 继承的应用状态。Warden 的普通 App 无需序列化；
JSON 仅描述预览配置。修改源码才需要重新构建。

视觉策略均先用 `testing::Driver` 重放一次，检查 present / absent 语义标签和有限布局坐标，
随后使用真实 wgpu 度量与渲染路径重放并截图。因此下面的视觉耗时包含两次重放；
仅语义策略省去的是整个第二遍和 PNG，并非与截图策略覆盖相同的工作。
仅语义 Driver 不是 wgpu 字体度量的替代，不用于证明像素或精确文字布局一致。

## 结果

七个 recipe 全部完成的墙钟时间（包含进程启动、JSON 读取、断言；视觉策略还包含渲染及 PNG 写出）：

| 策略 | 五轮中位数 | 最小–最大 |
|---|---:|---:|
| 独立进程串行 | 2.360 s | 2.318–2.372 s |
| 独立进程并行（2 workers） | 1.280 s | 1.266–1.300 s |
| 单进程、独立 renderer | 1.725 s | 1.653–1.783 s |
| 单进程、共享 renderer | 1.130 s | 1.113–1.167 s |
| 单进程、仅语义断言 | 0.264 s | 0.261–0.266 s |

共享 renderer 相比逐例启动减少约 52% 的总耗时；相比单进程逐例初始化后端减少约 34%。
本组两个独立 worker 的 GPU/字体初始化仍重复，吞吐没有超过共享 renderer。
这不表示更多 CPU 型案例或其他机器并行也无效。

这些值不含编译；此前 Warden UI 源码增量构建约 1.4 秒，两项来自不同实验，不冒充实测端到端时间。

## 隔离与失败实验

- 四种视觉策略、五轮、每轮七图，检查完整 PNG 的 SHA-256 一致；不是只检查文件存在。
- 共享 renderer 反序运行七个场景，截图仍须与独立进程一致。
- 故意插入找不到目标的命令，以及错误的语义断言；两个案例分别报告失败，
  后续正常案例继续执行，整个进程以非零状态退出。
- 框架 GPU 回归另外覆盖跨 recipe 的尺寸、DPI、计数初始状态、虚拟时钟、输入重置，
  以及无效版本、尺寸和命令失败后的恢复。

```sh
cargo test --locked -p plico-desktop --no-default-features --features wgpu \
  --test preview_session -- --ignored
```

这验证的是普通 recipe 错误。进程崩溃、GPU device loss 和无限循环不能依靠同进程隔离；
此类案例仍需要进程边界和外部超时。

## 当前建议

默认采用「编译一次 → 一批 JSON recipe → 每例新 App → 共享 renderer」。
快速反馈先运行语义断言；需要验收外观时再运行相关视觉 recipe。
完整截图集适合作为一轮修改后的回归，不必每次改 JSON 都走 Cargo。

一条 recipe 表示一个独立起点和一段相关交互，例如「打开主界面 → 搜索 → 断言筛选结果」。
不要把所有测试拼成依赖前序状态的长旅程。直接 fixture 用于覆盖难以抵达的显示状态；
交互步骤用于验证状态转换，两者都保留。

当前实验 runner 支持末尾的标签存在/不存在断言、案例名称、阶段耗时、独立 PNG 和失败后继续。
尚未提供步骤中间断言、图片 baseline 管理、异步服务完成条件、常驻 IPC 或通用 CLI 批处理。
批内采用同一套固定字体环境；跨机器像素一致、真实窗口输入、IME、剪贴板和真实账户不在结论内。
更大套件应测量内存/cache 增长，并使用有限批次或有限数量 worker，而不是无限常驻。
