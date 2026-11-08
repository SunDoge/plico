# Cargo 构建性能指南：Plico 实测

> Warden 应用与应用 benchmark 已迁到独立 Plicowarden 仓库；下文为迁移前的实验记录。重跑应用脚本需在 `../plicowarden` 中使用新包名 `plicowarden`。见 [仓库边界](../plicowarden.md)。

2026-11-08，对照 [Cargo 构建性能指南](https://doc.rust-lang.org/cargo/guide/build-performance.html)，
测量当前默认 wgpu Gallery 的真实源码修改→构建完成时间。

结论：优先保留框架/renderer 优化，只将应用改为 opt0，并减少整个依赖图的调试信息。
本轮稳定版最佳组合约 1.25 秒，原配置约 3.74 秒。单独减少应用的调试信息收益很小；
关闭第三方依赖调试信息、将 workspace 调试信息降到行号，才进一步降低链接负担。
实验阶段没有改变默认配置。随后已采用这组开发配置：workspace 保留行号，第三方依赖
关闭调试信息，Gallery 与 Warden 应用使用 opt0；见根 Cargo.toml 和 contributing.md。
复测脚本显式指定各组 debug 选项，避免采用新默认值后改变历史基线。

## 方法与边界

- Linux x86-64，Intel Core Ultra 9 285K；stable rustc 1.99.1 / LLVM 23.1.1。
  nightly 为 rustc 1.101.0-nightly（`2d20b42b8`, 2026-11-04）/ LLVM 23.1.5。
- 将当前工作区文件复制到忽略的 reports 目录，记录源文件 SHA-256；不修改原应用源码。
  stable/nightly 各有独立 target，每种配置复用本组共同依赖缓存。
- 直接调用 `rustup which` 返回的 Cargo/rustc，清除 compiler wrappers、继承的 rustflags
  和 profile overrides，绕过 mbx，显式 `CARGO_INCREMENTAL=1`。
- `build --locked --offline -j 6 -p gallery --bin gallery --message-format=json`。
  每组先编译依赖/配置，再做一次不计时的源码修改构建；不把切换配置或准备依赖算成增量样本。
- 每组 5 次文案修改、5 次布局表达式修改，交替改变两类样本的先后顺序；每次内容不同。
  文案改变 workspace 页标题；布局将首个 gap 改为显式 `Length::Px` 表达式并使用不同偏移。
  布局样本不是只替换一个同类型整数常量，和文案样本分别统计。
- 每次检查 Cargo JSON：必须只重编译 Gallery lib + bin，不允许混入依赖重编译。
  记录墙钟时间，包含 Cargo、编译和最终链接；不含应用启动与截图。
- 每组采样后恢复副本源码、重新构建，再执行 Controls 与 Workspace 的现有 JSON 交互场景。
  比较 PNG IHDR 与解压后的扫描行；相同指纹保证像素一致，不受压缩字节差异影响。
- 每种工具链在实验首尾重复相同基线。没有固定编译 CPU 频率或隔离其他桌面程序；
  五次中位数不是冷构建、尾延迟或其他平台的保证。mbx 日常路径的绝对耗时仍可能不同。

## 稳定版结果

框架保持 opt1、第三方依赖保持 opt2。除特别注明，框架/第三方调试信息均为 debug2。
“指南调试信息策略”是 workspace 使用 `line-tables-only`、第三方依赖 `debug=false`。
所有调试断言和 overflow checks 保持开发模式默认值。

| 配置 | 文案中位数 / 秒 | 布局中位数 / 秒 | 可执行文件 / MiB |
| --- | ---: | ---: | ---: |
| 原配置：应用 opt1/debug2 | 3.735 | 3.739 | 817.6 |
| 只将应用改为 opt0 | 1.663 | 1.659 | 823.4 |
| 应用 opt0/debug1 | 1.632 | 1.623 | 812.9 |
| 应用 opt0/仅行号 | 1.628 | 1.627 | 808.8 |
| 应用 opt0/无调试信息 | 1.596 | 1.604 | 805.0 |
| 应用 opt0/debug1，第三方依赖 debug0 | 1.329 | 1.346 | 335.9 |
| 应用 opt0 + 指南调试信息策略 | 1.245 | 1.260 | 188.5 |
| 原配置回测 | 3.733 | 3.749 | 817.6 |
| 只采用指南调试信息策略，保留应用 opt1 | 2.972 | 2.947 | 184.8 |

单独应用指南调试策略，约减少 21% 构建等待；只关闭应用优化，约减少 56%；组合约减少 66%。
应用自身 debug2→debug1 的额外收益很小，不宜把旧实验的百分比直接叠加到当前结果。
回测原配置仍约 3.74 秒，没有观察到足以解释组合收益的整体负载漂移。

当前根 Cargo.toml 的注释声称不生成依赖调试信息，但 `[profile.dev.package."*"]`
实际只有 `opt-level=2`，没有 `debug=false`；贡献文档也描述了尚未出现在当前 manifest
中的 dependency debug/strip 设置。本实验依据真实配置和 Cargo artifact profile，不依据注释。
单独关闭第三方调试信息后，文件约从 813 MiB 降到 336 MiB；再精简 workspace 调试信息，
降到约 189 MiB。切换依赖 profile 的准备构建约 114 秒，未计入增量结果；首次切换需要付出重建成本。

## nightly 单项对照

固定应用 opt0/debug1、框架 opt1/debug2、第三方 opt2/debug2，仅对 Gallery 应用包改变
后端、前端线程数或链接器。其余依赖维持 LLVM/默认前端/LLD，避免重编依赖污染增量样本。
使用 nightly 的 profile rustflags / codegen-backend 开关；这不是把整个 workspace 切到
Cranelift 或并行前端的实验，也没有把这些选项与最优调试信息配置组合后再测。

| 配置 | 文案中位数 / 秒 | 布局中位数 / 秒 | 可执行文件 / MiB |
| --- | ---: | ---: | ---: |
| LLVM + LLD 基线 | 1.621 | 1.620 | 810.3 |
| 应用前端 threads=8 | 1.675 | 1.668 | 810.3 |
| 应用 Cranelift | 1.603 | 1.618 | 809.0 |
| 应用使用 clang + Wild 0.10.0 | 1.583 | 1.551 | 809.8 |
| LLVM + LLD 基线回测 | 1.613 | 1.634 | 810.3 |

相对同版本 LLVM 基线，前端 8 线程没有观察到加速，反而略慢；Cranelift 在应用已经
opt0 后几乎没有额外收益。不能据此推断它对 opt1 或冷构建无效。
Wild 的文案中位数仅少约 0.04 秒、布局少约 0.07 秒；相对基线回测也只是几十毫秒。
本轮五次样本不足以把这样的小差异当作稳定保证，不值得仅为此切换默认工具链。
`readelf .comment` 确认基线确实为 LLD 23.1.5、Wild 组确实为 Wild 0.10.0。

## 运行时代价与正确性

复用 [CPU probe 源码](dev-build-probe.rs)，临时在副本中注册为普通 bin，避免 example
引入 dev-dependencies。构建时间不计入运行计时。固定逻辑 CPU 0；三轮正反次序运行三种配置。
每进程每页预热 100 次、采样 3000 次；下表是三轮各自 p50 的中位数，单位微秒。
`Source::build_frame` 每次显式 request_rebuild，使用固定 VirtualClock 与保留的引擎缓存。

| 页面 | 原配置 | 应用 opt0/debug1 | 应用 opt0 + 指南调试策略 |
| --- | ---: | ---: | ---: |
| workspace | 25.45 | 34.05 | 34.13 |
| controls | 33.02 | 42.04 | 42.35 |
| layout | 24.31 | 32.59 | 32.65 |
| data-table | 30.34 | 42.38 | 42.41 |
| tree | 32.27 | 45.45 | 45.61 |
| motion | 20.23 | 27.83 | 27.94 |

三种配置每页的节点数量 checksum 相同。应用 opt0 在这些页面增加约 8–13 微秒，
约 27–41%；本项准备工作仍低于 0.05 ms。调试信息策略没有显示出额外的明显运行成本。
本 probe 没有原生文字 provider、GPU 绘制或窗口输入，不是完整帧性能、FPS 或功耗结论。
nightly 变体的 CPU 开销没有在本轮测量。

本轮共 14 组（含基线回测）、140 个有效增量构建样本；全部只重编译 Gallery lib + bin。
28 次交互截图全部完成，Controls 和 Workspace 在全部 stable/nightly 配置间分别像素一致。
这些检查覆盖指定场景的声明、布局与绘制，不代表全部页面和平台都已验收。

## 实际建议与复测

本轮最佳稳定版配置可以临时使用：

```sh
cargo run -p gallery \
  --config 'profile.dev.package.gallery.opt-level=0' \
  --config 'profile.dev.debug="line-tables-only"' \
  --config 'profile.dev.package."*".debug=false'
```

框架 opt1、第三方 opt2 保持原值。需要调试局部变量/复杂类型时应恢复相应包 debug2
并重建。这里只给出经过实验的开发选项，没有改变 dev/release 默认值。

[复测脚本](../../scripts/bench-build-performance.py) 只依赖 Python 标准库和本机 Rust 工具链；
nightly 组另需匹配的 Cranelift 组件、clang 与 Wild（默认 `~/.cargo/bin/wild`，可用
`PLICO_BENCH_WILD` 指定其他路径）。已有完整样本会复用，严格重测请选择新的输出目录。

```sh
PLICO_BENCH_OUTPUT=reports/build-performance-new python3 scripts/bench-build-performance.py
PLICO_BENCH_OUTPUT=reports/build-performance-new PLICO_BENCH_GROUP=nightly \
  python3 scripts/bench-build-performance.py
PLICO_BENCH_OUTPUT=reports/build-performance-new PLICO_BENCH_RUNTIME_ONLY=1 \
  python3 scripts/bench-build-performance.py
```

可用 `PLICO_BENCH_VARIANTS` 选择逗号分隔的配置名，`PLICO_BENCH_SAMPLES` 改变每类样本数。
脚本保存原始日志、Cargo artifact 记录、源文件哈希、截图与 JSON；生成产物都在忽略的 reports 下，
不纳入提交，本文不依赖其链接。

本轮没有删除依赖/feature，也没有测试 workspace feature-unification：它们主要针对
首次构建或切换构建目标后的重复依赖编译；本轮有效增量样本已经确认没有重编译依赖，
不能用这组结果推断这两类优化无效。更早的 Skia 后端实验见 [Cranelift + Skia](cranelift-skia.md)，
不可直接和本轮不同工具链、后端、配置的数字比较。

## 先前 mbx 路径的阶段定位

在同日更早的真实 mbx 增量命令上直接附加 `-Ztime-passes` 采样，Gallery 库总计约 2.6 秒，
其中等待 LLVM 完成优化/代码生成约 2 秒；类型检查约 0.03 秒、借用检查约 0.013 秒，
最终 binary 的外部链接约 0.65 秒。阶段计时嵌套且可并行，不能全部相加。
关闭应用本地 ThinLTO 仅将库编译降到约 2.05 秒，关闭应用优化则约 0.52 秒。

mbx 在 Cargo verbose 命令之后注入私有 incremental 路径；缺少可见的 `-C incremental`
不代表未启用增量。普通 `cargo rustc -- -Ztime-passes` 会改变 mbx 策略，本轮矩阵因此统一
绕过 wrapper，另外保留真实 mbx 子进程的阶段采样作为瓶颈定位证据。
