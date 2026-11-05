# Gallery 开发编译配置实验

2026-11-05，本机 Intel Core Ultra 9 285K、Linux x86_64、rustc 1.99.1。
实验使用 `10ca4c8` 加当时工作区移除 `paint-scene` 开关的源码快照，
在忽略目录内独立复制构建，不改动开发工作区源码或复用它的 target。
这里比较的是应用开发配置，不是 renderer 性能，也没有测其他平台。

## 编译协议与结果

三组共同配置：框架 workspace crates 为 dev opt 1、debug 2；
所有非 workspace 依赖为 opt 2、debug 0、strip debuginfo；incremental=true。
只有 Gallery 的 opt-level/debug 不同，debug assertions 和 overflow checks 保持开启。

使用 `rustup which cargo` / `rustup which rustc` 的真实工具链，清空 Rust compiler
wrapper 和环境里的 profile overrides，绕过 mbx action cache，`--locked --offline`，
4 个 Cargo jobs；三组共用独立 target 目录中的依赖产物。首次完整依赖预热耗时
171.783 秒，不能拿它与后续增量样本比较，也不能据此声称 app opt 0 加速了冷构建。
每次切换组先做一次不计入统计的重编译，再测无修改、文字修改、布局修改。
三组顺序逐轮正反交替，每个场景每组 5 次；下表为墙钟时间中位数，单位秒。

| Gallery 配置 | 文字修改 | 布局修改 | 无修改 |
| --- | ---: | ---: | ---: |
| opt1-debug2 | 2.958 | 2.953 | 0.275 |
| opt1-debug1 | 2.639 | 2.553 | 0.263 |
| opt0-debug1 | 1.358 | 1.298 | 0.267 |

文字修改替换 `apps/gallery/src/lib.rs` 侧栏的 `plico` 标签；布局修改改变同一侧栏
`.padding(12)` 的数值。每个样本使用新内容，避免重复构建同一输入。两种修改均经
Cargo JSON artifact 消息确认只重编译 Gallery lib + bin（共 30 次），不重编译框架
或外部依赖；无修改的 15 次均没有重新生成 artifact。计时包含 Cargo 启动、UI 宏展开、
应用 codegen 和最终链接，不包含应用启动、窗口绘制或 watcher debounce。

opt 0/debug 1 相比 opt 1/debug 2，文字修改缩短约 54%，布局修改约 56%；
同为 debug 1 时，降低优化等级另外节省约 49%。单独精简应用调试信息约节省 11–14%。
opt 0 布局修改范围 1.266–2.179 秒，其余配置也有波动；这是本机交替采样结果，
不是其他机器或任意应用的延迟保证。没有隔离其他桌面程序，故记录范围而非只报最快值。

## 运行时代价

使用 [CPU probe 源码](dev-build-probe.rs)，不创建 GPU 或窗口。每个页面使用
相同初始状态、1120×820、注入的固定 VirtualClock；每次显式 `request_rebuild()`
后调用真实 `Source::build_frame`，保留框架、协调和布局缓存。TextContext 使用默认
文字路径，没有配置 wgpu 原生文字 provider；这不是完整 wgpu 帧耗时。

三组均预先构建 probe 二进制，固定在逻辑 CPU 0，正反交替执行 6 轮。
每进程每页面预热 100 次、采样 3000 次；下表为六轮各自 p50 的中位数，单位微秒。
所有配置的各页面节点数 checksum 一致；没有以 FPS 或整机 CPU 监测替代这项计时。

| 页面 | opt 1/debug 2 | opt 1/debug 1 | opt 0/debug 1 |
| --- | ---: | ---: | ---: |
| workspace | 17.75 | 17.73 | 22.22 |
| controls | 26.13 | 26.16 | 32.08 |
| layout | 19.32 | 19.32 | 24.53 |
| data-table | 25.26 | 25.36 | 34.10 |
| tree | 26.91 | 26.88 | 36.74 |
| motion | 16.34 | 16.26 | 19.99 |

应用 opt 0 的准备路径增加约 4–10 微秒（约 23–37%）；这六个中小页面仍低于
0.04 ms。debug 1 与 debug 2 的运行耗时基本相同。结果支持在 Gallery UI 快速迭代时
使用 opt 0/debug 1，不支持把整个框架或所有依赖降到 opt 0，也不能外推 ESP32。
数据未覆盖文本大面积失效、窗口输入延迟、GPU 绘制或完整动画 CPU 占用。

## 如何使用和复测

不改 manifest 就能试用应用级快速开发配置：

```sh
cargo run -p gallery \
  --config 'profile.dev.package.gallery.opt-level=0' \
  --config 'profile.dev.package.gallery.debug=1'
```

首次切换配置的构建不能作为热增量样本。依赖 profile 改动也会触发重新构建。
若需固定配置，可把两个设置放入 `[profile.dev.package.gallery]`；本实验没有
自动更改应用默认 profile，其他应用需独立测量。

复测时在独立 checkout 使用共同依赖配置，并分别传入上表三个 Gallery profile。
用 `--message-format=json` 检查 compiler-artifact 的 fresh 字段；每组预热后分别
替换标签字符串和 padding 值（每轮用新值），以进程单调时钟计时。不要 `cargo clean`，
不要将 `cargo check` 结果当成带链接的 UI 开发构建时间。

运行 CPU probe 时，在独立 checkout 中复制：

```sh
cp docs/experiments/dev-build-probe.rs apps/gallery/examples/dev_build_probe.rs
cargo run -p gallery --example dev_build_probe \
  --config 'profile.dev.package.gallery.opt-level=0' \
  --config 'profile.dev.package.gallery.debug=1'
```

实际采样应先 build，再固定核心直接执行生成的程序；不要把 Cargo 编译耗时算进运行计时。
Gallery example 会引入 `plico-editor` 等 dev-dependencies，首次 probe 构建因此还有额外
依赖准备；日常迭代优先构建 Gallery bin，而不是所有 examples/all-targets。
这部分开销不计入上面的 30 次应用 bin 修改样本。

截图、原始日志、逐次耗时与 JSON 汇总属于本地 reports 产物，不参与提交。
