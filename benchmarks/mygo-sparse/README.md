# MyGO：稀疏更新的宽表实验

这是可重建的诊断实验，不是跨框架胜负测试，也不是 MyGO 源码补丁。不进入 CI。

## 场景与方法

CSV / 数据库结果查看器，1280×900 DIP，10,000 或 100,000 条记录，使用 MyGO 自带
`Table` / `ListState` 的行虚拟化。只构建约 28 个可见及预取行，不遍历整个数据集。
表格包含 4–512 列，每列宽 78 DIP；窗口约能显示 16 列，其余可以横向滚动查看。
宽 CSV、业务数据库导出和模型特征表会遇到这种情况；256–512 列属于压力档，不代表
普通设置页或日常十来列的表格。

四种工作负载：

- `host-cell`：每次只改第一个可见单元格。
- `host-status`：表格完全静止，只改表格外的一处状态文字。
- `scroll`：用真实 `PointerScroll` 事件每帧滚动 8 DIP，方向定期反转。
- `paint-animation`：使用 MyGO 支持的 `Painter.AnimationFrame`，只让一根小条运动，
  不调用应用状态变更入口。测试断言此路径不重新执行 view；这是已有优化的负对照，
  不能假定 MyGO 的每种动画都重建全树。

文本预先准备，每次只在两个已预热值间切换；没有每帧生成整个数据集、排序、解析、
网络、sleep 忙等、图片解码等人为重操作。预热 30 帧，先强制提交相同的虚拟行窗口，
再进入 paint-only 路径。测试检查状态文字确实进入当前节点树及行构建数量有界。

通过 Go overlay 把测试注入 MyGO 的 `ui` 包，使用其真实声明、布局和 Scene 构建路径。
宿主的 present 只保存 Scene，**省略 CPU 光栅化、GPU、实际窗口及合成器**，所以这里
不是窗口实测 FPS；帧准备吞吐和 16.67ms 超预算比例反映 CPU 侧的限制。
`frameStats` 记录 build/layout/paint；`declare` 只测应用 view 调用，属于 build 的子集，
不能与 build 相加。小部分虚拟行也可能在 layout 阶段补建。

## 复现

需要 Linux、MyGO checkout 所需版本的 Go、jq，以及其原生文字依赖。所有框架源码保持
原样，overlay / test binary 是临时或本地 reports 产物；测试使用生产配置
`mygo_noinspector`、`CGO_ENABLED=0`、`GOMAXPROCS=1`。

```sh
# 从 Plico 根目录运行。CPU 7 换成自己允许绑定的 CPU。
MYGO_DIR=/path/to/mygo BENCH_COUNT=3 BENCH_TIME=60x \
  BENCH_PATTERN='BenchmarkPlicoSparseTable/cols=(128|256|512)$/rows=100000$/(host-status|scroll)$' \
  taskset -c 7 benchmarks/mygo-sparse/run.sh

# 有节奏的 60Hz 更新，实际测量进程 user+system CPU，预热/编译不计入测量区间。
MYGO_DIR=/path/to/mygo MYGO_SPARSE_CPU=1 BENCH_COUNT=1 LOAD_TIME=3s \
  taskset -c 7 benchmarks/mygo-sparse/run.sh

# CPU profile。输出目录必须已存在；文件不提交。
mkdir -p reports/mygo-sparse
MYGO_DIR=/path/to/mygo BENCH_COUNT=1 BENCH_TIME=3s \
  BENCH_PATTERN='BenchmarkPlicoSparseTable/cols=256$/rows=100000$/scroll$' \
  CPU_PROFILE="$PWD/reports/mygo-sparse/scroll.cpu" \
  taskset -c 7 benchmarks/mygo-sparse/run.sh
go tool pprof -top -cum reports/mygo-sparse/scroll.cpu.test reports/mygo-sparse/scroll.cpu
```

CPU 测试用 ticker 合并过期 tick，不累积事件队列。`cpu_onecore_pct` 为测量区间内
进程 user+system CPU 时间除以墙钟时间；包括 Go GC 和原生文字调用。
`cpu_all_24cores_pct` 再除以主机在线逻辑 CPU 数，是等权归一化值，不包含其他进程。
CPU affinity 不改变这个主机分母；可通过 `HOST_CPUS` 显式指定。
这不是功耗测量，也不意味着异构 CPU 的每个逻辑核性能相同。

overlay 使用 MyGO 私有测试入口；升级 MyGO 后可能需要调整 fixture。脚本在 stderr
记录 checkout commit，不自动拉取、修改或升级 MyGO。

## 本机结果（2026-10-29）

MyGO `7e8688284e4b1fc752169ed9db5f51a5f213a2e6`，干净 checkout；Go 1.27.3，Linux
amd64，Intel Core Ultra 9 285K，24 个逻辑 CPU，绑定 CPU 7。

固定 100,000 行，60 次更新/轮，三轮取 p50/p95 的中位数；以下全部新文字整形次数为零：

| 列数 | 操作 | p50 | p95 | 超过 16.67ms 的比例 |
| --- | --- | ---: | ---: | ---: |
| 128 | 只改状态栏 | 5.80ms | 6.96ms | 0% |
| 128 | 连续滚动 | 6.93ms | 9.16ms | 0% |
| 256 | 只改状态栏 | 18.53ms | 20.55ms | 100% |
| 256 | 连续滚动 | 19.62ms | 21.11ms | 100% |
| 512 | 只改状态栏 | 43.12ms | 44.96ms | 100% |
| 512 | 连续滚动 | 49.07ms | 51.51ms | 100% |

256 列只改状态栏时保留约 14,913 个 UI 节点，512 列约 29,761 个。
256 列的 view 声明均值约 7.15ms、layout 约 8.01ms、paint 约 1.70ms；512 列的
view 声明已约 18.56ms。这不是扫描 100,000 行造成的：10,000 行对照也只构建同样
数量的行，初测耗时接近。

节奏控制的 CPU 测试，每轮测量 3 秒；12–256 列取三轮中位数，512 列取两轮中位数。
这里是持续更新的进程 CPU，不能和上面不等待 tick 的微基准绝对耗时直接混用：

| 列数 | 只改状态栏：单核 CPU | 24 核归一化 | CPU 准备吞吐 | paint-only 对照：单核 CPU |
| --- | ---: | ---: | ---: | ---: |
| 12 | 11.5% | 0.48% | 60.0 帧/s | 6.1% |
| 64 | 20.0% | 0.84% | 59.9 帧/s | 10.2% |
| 128 | 41.7% | 1.74% | 59.9 帧/s | 12.5% |
| 256 | 99.5% | 4.14% | 57.7 帧/s | 15.4% |
| 512 | 99.5% | 4.14% | 22.5 帧/s | 20.0% |

CPU 单核接近饱和时，ticker 丢弃过期 tick，实际执行次数低于请求的 60Hz；表中吞吐
不是实际窗口呈现 FPS。相反，CPU 准备阶段已不足以维持 60Hz 时，增加 GPU / 窗口
成本不会自动弥补这部分缺口。没有录制真实窗口或测量 input-to-photon 延迟。

## 归因与限制

256 列滚动的约 4.3 秒 CPU profile：`flexLayout` 累计约 28.8%、`commitElement`
约 12.1%、`lookState` 约 10.7%；`finalizeInputs.func6`（反复检查尚未完成的
waiting actions）累计约 14.4%。表格构建路径累计约 32.8%。这些累计占比存在重叠，
不能相加；profile 包含预热和测试设置，百分比用于定位，不作为严格阶段占比。

结果支持：**稀疏更新确实会让大量静态 UI 的重新声明、输入收尾及布局成为瓶颈**。
但不能把全部耗时归因于抽象的“一次树遍历”，也不是 GC 或文字首次整形造成的。
MyGO 已有行虚拟化和 paint-only 快路；这里尤其暴露了横向未虚拟化及实现细节的成本。
横向虚拟化、改善 waiting actions 扫描、局部 retained 更新都值得考虑，并非必须换
成 reactive 才能修复。

此实验没有实现同场景 Plico，因此不宣称 Plico 已经更快。Plico 的属性级 reactive
可以减少声明范围，但文字更新仍可能触发布局和完整 flatten，必须继续测量这些共享
成本。普通小表格未表现出同等级的帧预算问题；低频状态更新的平均 CPU 也会更低。

## 文字很多、只有一段变化的补充实验

`BenchmarkPlicoSparseParagraphs` 使用 760×900 的文档窗口和 64 / 512 / 2048 / 5000 段
不同的普通文字，每段约 170 个英文字符，第一段模拟持续收到新 token。所有输入字符串
均预生成，宽度与字号固定，不重新解析 Markdown。另测文字完全不变但请求一次完整帧的
对照；并用自带 List 对照同一份历史。只改第一段时，其他段落的内容与样式都不变。

```sh
MYGO_DIR=/path/to/mygo BENCH_COUNT=3 BENCH_TIME=10x \
  BENCH_PATTERN='^BenchmarkPlicoSparseParagraphs$' \
  taskset -c 7 benchmarks/mygo-sparse/run.sh
```

每轮预热 6 次、测量 10 次，三轮平均耗时取中位数；同机结果：

| 历史大小 | 写法 | 操作 | 平均 CPU 帧耗时 | 每帧新文字布局 |
| --- | --- | --- | ---: | ---: |
| 512 段 | 普通 Scroll | 只改第一段 | 0.69ms | 1 |
| 2048 段 | 普通 Scroll | 只改第一段 | 1.63ms | 1 |
| 5000 段 | 普通 Scroll | 只改第一段 | 547.29ms | 10001 |
| 5000 段 | 普通 Scroll | 全部不变、请求帧 | 544.56ms | 10001 |
| 5000 段 | List 虚拟化 | 只改第一段 | 0.53ms | 1 |
| 5000 段 | List 虚拟化 | 全部不变、请求帧 | 0.47ms | 0 |

表中“新文字布局”对应 `internal/text.System.LayoutsMade()` 的增量，即未命中布局缓存而
创建的文字布局对象；不是独立测量的 HarfBuzz / Pango 整形函数调用次数。fixture 输出
字段为 `text-layouts/op`。相同文本可因测量约束不同创建多个布局，所以 10001 不代表
有 10001 个不同段落。

这里测到了**缓存工作集越界后反复淘汰、重建**，不是“一段改动把所有其他段落标脏”。
本版本文字缓存最多 4096 个条目，普通预算 8MiB、活跃布局硬上限 64MiB。2048 段的
不变文字仍能复用；5000 段这组工作集超过限制，完整布局扫描会反复逐出后续马上需要
的条目。该例约 545ms 花在 layout 阶段，分配请求约 266MB/帧；并非 GPU 光栅化造成。

完全不改文字也同样慢，说明问题是触发完整帧后反复访问超出缓存预算的工作集。
List 每帧只创建约 19 个段落节点，工作集缩小后恢复正常。官方 `markdown-chat` 示例
本就使用 List；这不是其官方聊天示例的性能结果，也没有测试 `markdown.View` 自己的
段落缓存或 editor。不能故意不用已有虚拟化，再宣称该框架所有长文档都会卡顿。

对 Plico 的启发是：保留未变节点的文字测量结果、缩小缓存工作集、使用虚拟化都很重要。
Reactive 可减少无关区域执行，但这个案例首先说明缓存与布局策略的问题；合理保留缓存的
immediate backend 也可能避免它。仍需同场景比较才能宣称两者性能差异。

## 真实窗口演示

```sh
MYGO_DIR=/path/to/mygo benchmarks/mygo-sparse/window.sh
```

脚本构建并启动真正的 MyGO 原生窗口，使用正常 native renderer；不是截图回放、
headless Tester 或浏览器。默认 1440×1060，窗口内的 1000 个 Text 都能看到。
二进制写入本地 `reports/mygo-sparse/mygo-sparse-window`，可直接再次运行。
`SPARSE_BUILD_ONLY=1` 只构建。该演示目前只验证 Linux。

顶部四个场景：

1. **1000 visible texts**：25×40 个文字单元，只修改左上角的一个；布局与文字缓存保持开启。
   默认 60Hz，主要用于观察重复声明、布局和 scene 生成的 CPU，而非保证所有机器掉帧。
2. **Wide table**：256 列、100000 行的原生 Table，保留行虚拟化，只改状态文字。
3. **5000 paragraphs / Scroll**：长文档普通 Scroll，第一段持续变化，演示缓存工作集过大。
4. **5000 paragraphs / List**：同一份文档，使用自带虚拟化作为对照。

`State / Text` 使用真实 `Window.Invalidate()`；它本身合并重复请求，源数据保存在 atomic
槽位，不排队积累成千上万个 Update 回调。`Paint-only control` 使用自定义 Draw 和
`Painter.After`，跳过 view 重建；它不是 reactive 或完整的原生 Text 更新方案，尤其没有
同步改变对应可访问性文字。长文档的该对照仅更新独立状态标签，保持正文不变。

`Running` 暂停时后台 ticker 停止，paint-only 也不继续申请帧；`Step` 单步更新。
暂停后 HUD 保留最后一次活跃测量，不代表仍有同样的 CPU 消耗。

诊断 HUD 显示已完成帧的 CPU prepare、build/layout/paint、native present 墙钟时间、
滚动约一秒的完成帧率、实际进程 CPU、节点数、新文字布局数及实际 renderer 路径。
它通过额外的 overlay 宿主包装记录数据，原始 MyGO 源码不被编辑。
HUD 自身约 4Hz 更新，也可能创建少量文字布局；真实窗口的 CPU 包含 GTK、驱动和
光栅化线程，所以不能与前面的无 raster/GPU 基准绝对数值直接等同。

真实窗口已经用隔离 X11 / Xvfb 验证初次呈现、paint-only 不执行 build/layout、暂停不
持续申请帧，以及 Scroll / List 切换；也已在本机 Wayland 桌面以实际 GPU 路径启动。
截图、窗口日志和本地验证脚本仍只放在 reports，不进入 CI。
