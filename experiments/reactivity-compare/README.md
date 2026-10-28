# Immediate 与 Sycamore 更新成本实验

这是独立 workspace，不加入 Plico 的默认构建或运行时。实验比较同一合成 retained tree 上的声明驱动，使用 `sycamore-reactive = 0.9.4`、真实 Taffy 和 `plico-renderer::Scene`。它不是两个完整 GUI 框架的端到端对比，也不是 Plico reactive 实现。

## 复现

在仓库根目录运行：

```sh
export CARGO_TARGET_DIR="$PWD/target"
cargo build --release --locked --manifest-path experiments/reactivity-compare/Cargo.toml
mkdir -p reports/reactivity-compare
for round in 0 1 2; do
  PLICO_ROUND=$round target/release/plico-reactivity-compare > "reports/reactivity-compare/full-$round.csv" 2> "reports/reactivity-compare/full-$round.log"
  PLICO_PAINT=retained PLICO_ROUND=$round target/release/plico-reactivity-compare > "reports/reactivity-compare/retained-$round.csv" 2> "reports/reactivity-compare/retained-$round.log"
done
PLICO_PAINT=retained PLICO_COUNT_ALLOC=1 PLICO_ROUND=3 target/release/plico-reactivity-compare > reports/reactivity-compare/alloc.csv 2> reports/reactivity-compare/alloc.log
```

CSV 输出纳秒计时换算为微秒：total 从状态变更开始，包含更新、提交、Taffy 和 Scene；mutation 在 Sycamore 路径包含 `batch`、通知和 effect 执行，dispatch 则只统计取出结果。不能把 mutation 和 dispatch 分别当成同一含义的纯通知/调度成本。每项 12 次预热、60 次采样，三个进程轮换驱动执行顺序。p99 只有 60 个样本，不能视作稳定的尾延迟。分配统计单独运行，它有原子开销；次数与字节是 allocator 请求量，不是常驻内存。

四个驱动共用以下工作：更新同一个结构的 Taffy 树并处理相同 K 个属性变更，然后生成相同的 Plico Scene。每次状态更新后逐节点检查结果；每种工作负载结束后比较四条路径的全部 PaintCommand，保留绘制模式还会与完整重建的 PaintCommand 对比。

- `naive`：修改状态，构造临时 Vec，扫描并比较全部 N 个值。
- `full`：修改状态，复用缓冲，扫描并比较全部 N 个值。
- `dirty`：显式给出已变更节点，只访问 K 个值。测试保证标记完整；真实程序必须维护失效协议。
- `reactive`：每节点一个 signal 和一个 effect，变更放在 `batch` 里；effect 写入队列，统一提交到相同的树。

`paint` 改颜色，不触发布局。`layout` 改每个固定高度叶子的宽度并调用 Taffy `compute_layout`。所有节点平铺在一个 flex column 中，固定高度、位置稳定；这种结构既不是典型完整应用，也不覆盖文字整形、换行、嵌套 layout 传播。没有 GPU 提交、命中测试、事件、窗口、热重载和渲染同步。状态和输出仅有两种值，Scene 用彩色矩形模拟文字属性，不能代表真实文字工作量。

两种绘制策略：默认每次重新构建完整 Scene；`PLICO_PAINT=retained` 假设每节点一条可独立修改的绘制命令，仅更新 K 条。后一种是刻意理想化的上界，不是当前 Plico 的增量 Scene 实现；不能据此说 Plico 已有 O(K) 画面更新。结构变化、混合层叠、裁剪、透明度和父级位置变化会破坏这个简化假设。

每组另外运行真实 `plico::Engine::frame_declarations`，使用 N 个 keyed text，验证 `visited=N`、`text_writes=K` 和无增删，**只测声明**。它与合成树的字段和执行路径不同，不能直接和共享树总时间相加、相除或声称是同一场景的端到端对比。Sycamore 的 child scope 另测创建/销毁及订阅清理；不是完整的条件组件和列表增删基准。

## 一次本机结果（2026-10-28）

Intel Core Ultra 9 285K、Rust 1.99.1、release。数值是三轮各自 p50 的中位数（µs），未固定 CPU 或提供统计置信区间；相近结果不表示有稳定胜负。

10,000 节点，固定布局、颜色变化：

| K | 绘制策略 | naive | full | dirty | Sycamore |
|---:|---|---:|---:|---:|---:|
| 1 | 完整重建 | 654.09 | 628.93 | 644.64 | 646.28 |
| 100 | 完整重建 | 670.38 | 664.52 | 662.50 | 679.98 |
| 5,000 | 完整重建 | 658.03 | 658.74 | 657.46 | 1,200.04 |
| 10,000 | 完整重建 | 655.21 | 651.67 | 648.82 | 1,794.97 |
| 1 | 理想化局部绘制 | 7.28 | 4.13 | 0.13 | 0.20 |
| 10 | 理想化局部绘制 | 7.46 | 4.25 | 0.32 | 1.42 |
| 20 | 理想化局部绘制 | 7.83 | 4.64 | 0.38 | 2.88 |
| 50 | 理想化局部绘制 | 8.45 | 5.13 | 1.10 | 7.59 |
| 100 | 理想化局部绘制 | 9.51 | 6.61 | 1.78 | 15.97 |
| 5,000 | 理想化局部绘制 | 44.51 | 39.91 | 39.68 | 531.62 |
| 10,000 | 理想化局部绘制 | 83.72 | 79.84 | 80.70 | 1,187.62 |

同样 10,000 节点、修改 1 个节点宽度时，理想化局部绘制下四种驱动总时间的三轮 p50 中位数为 1,245–1,266µs；实际 Taffy compute 仍涉及大量工作。本树结构下只跳过声明或绘制不能解决布局成本。

分配单独采样、10,000 节点颜色变化：理想化局部绘制下，`full` 和 `dirty` 热路径均为 0 次；`naive` 约 1 次 / 160,000 字节。Sycamore K=1 为 1 次 / 32 字节，K=100 为 6 次 / 2,016 字节，K=10,000 为 13 次 / 262,112 字节。完整重建 Scene 则四种驱动都约 20,000 次、6.72 MB/帧（不含 naive 临时 Vec）；这是本实验逐矩形构造 path 的成本，不能当成 Plico 全部应用的分配数据。

真实 Plico 的声明基线在本次三轮中，10,000 节点 K=1 的 p50 为约 0.65–0.70ms，始终访问 10,000 个节点、只写一个文本；它不包括这里的 Taffy 和绘制。Sycamore 独立 scope churn 每次创建并销毁 1,000 个子 scope/effect，未计数模式一次测得 p50 约 234µs；分配计数模式约 2,000 次 / 86KB。清理后修改源 signal 不再触发已销毁 effect。

这组数据支持“真正稀疏且后端也能局部工作时可节省前端遍历”，也支持“密集 effect 调度并非免费”；但没有证明真实 Plico 端到端的交叉点，更没有证明 Plico 可以安全实现局部 Scene 更新。还需测试相同真实树与 adapter 的局部 patch、文本 shaping、虚拟列表、插删/重排、条件分支、事件及动画，以及不同依赖图拓扑、常驻内存和真实应用。

## 真实 Plico 管线：不生成 Scene 的更新成本

第二个程序 `real_pipeline` 比较实际 `Source` 和 `Adapter`，使用同一 `TextContext`、Taffy 布局和文本处理。不提交 GPU；默认计时也不构建 Scene。每条路径有独立 Plico Engine 与 retained tree：一条重新执行整个 `View::show`；另一条每个文本节点对应一个 Sycamore 0.9.4 signal/effect，批量更新后将 effect 结果写入队列，再按预先保存的 Entity 逐个 patch 节点。这是人为添加的、默认关闭的 `experiment-patch` 功能，仅限现存纯文本的内容和前景色，不是公开的通用 reactive API，也不覆盖事件、结构变化、owner、主题或生产用热重载。

两条路径都进入相同的节点同步和布局管线；同样更新应用状态，计入响应式通知、队列、节点 patch。颜色场景不应重排，文字场景使用两种英文文本，会触发真实测量与布局。1,000 / 10,000 个节点都放在一个平铺 flex column 里，视口足够大以覆盖全部节点，代表有意构造的大树，而不是已虚拟化的列表。使用虚拟时钟，每组预热 5 次、采样 20 次，交替执行顺序。逐帧检查状态；80 节点案例每次更新后、所有案例结束后，在计时外构建 Scene 并比较绘制命令。Scene 使用同一文字上下文以避免两次加载字体产生不同资源 ID；这也意味着文字缓存共享。绘制命令比较不是像素或完整行为验证。

```sh
CARGO_TARGET_DIR="$PWD/target" cargo build --release --manifest-path experiments/reactivity-compare/Cargo.toml --bin real_pipeline
for round in 1 2 3; do
  target/release/real_pipeline > "/tmp/plico-real-$round.csv"
done
# 可选：把 CPU Scene 构建计入测量；仍不提交 GPU。
PLICO_WITH_SCENE=1 target/release/real_pipeline > /tmp/plico-real-scene.csv
```

2026-10-28 本机 release 测量，下面是三轮各自 p50 的中位数，单位毫秒；未固定 CPU，样本量较小，交叉范围只用于描述本工作负载。

| 10,000 节点，更新数 K | 文字：完整声明 | 文字：Sycamore + patch | 颜色：完整声明 | 颜色：Sycamore + patch |
|---:|---:|---:|---:|---:|
| 1 | 18.68 | 16.93 | 4.77 | 4.26 |
| 100 | 17.54 | 16.27 | 5.47 | 4.71 |
| 1,000 | 20.40 | 19.39 | 9.21 | 8.01 |
| 2,000 | 24.30 | 23.48 | 9.62 | 10.17 |
| 5,000 | 31.20 | 32.99 | 17.74 | 18.92 |
| 10,000 | 42.58 | 47.05 | 26.93 | 28.72 |

在此测试中颜色场景的交叉范围约在 K=1,000–2,000，文字场景约在 K=2,000–5,000。它们不是框架通用阈值：共享管线可能成为主成本，节点树形状、变更内容、signal 依赖拓扑、缓存和机器负载都会影响结果；K 只是发生变化的文本节点数，并非所有 reactive signal 的数量。颜色场景 `layout_runs=0`，不意味着其余管线就是 O(K)。当前测量不包含动态列表、结构增删、内存常驻量、用户交互和真实 GPU 时间，因此不能单靠此结果决定是否切换架构。

## 显式依赖元组：无捕获 watch 和事件回调

`src/lib.rs` 是独立的单线程 PoC，不接入正式 Plico API。`src/bin/explicit_watch.rs` 包含 headless counter 演示和真实 Plico CPU 管线对比：

```rust
let mut rt = Runtime::default();
let count = rt.state(|| 0);
let step = rt.state(|| 2);

let label = rt.watch((count,), |cx, count| {
    cx.text(format!("Count: {}", *count.read()));
});
let summary = rt.watch((count, step), |cx, count, step| {
    cx.text(format!("Count {}, step {}", *count.read(), *step.read()));
});
let increment = rt.on_click(label, (count,), |count| {
    count.update(|c| *c += 1);
});

// 可选：权限直接使用 Rust 引用表达。类型由依赖元组推导。
let by_step = rt.on_click_rw(label, (step, count), |step, count| {
    *count += *step; // step: &i32，count: &mut i32
});

rt.flush();
rt.click(increment);
rt.click(increment);
rt.flush(); // 两次写入，每个依赖区域只执行一次
rt.unmount(label);
assert!(!rt.click(increment)); // 旧 handler 失效
```

这个版本不用 proc macro，`watch` 已经是普通函数：`watch<D: WatchDependencies>(deps: D, callback: D::Callback)`。一、两个依赖的元组实现分别将关联类型 `Callback` 定义为对应参数个数的 `fn(...)`；Rust 根据依赖元组推导闭包参数，并将非捕获闭包转换为函数指针。隐式捕获外部变量无法编译。`watch!` 仅作为普通函数的可选转发别名。一个依赖的实际事件参数类型为 `fn(Access<'_, T>)`，两依赖的读写权限版本为 `fn(&A, &mut B)`。目前元组只支持一、两个依赖，两个事件依赖使用 `on_click2` 或 `on_click_rw`；没有实现任意元组长度的统一 trait 或 button builder 集成。

运行时保存 typed state handles、带代际的 region/handler key、静态依赖边和函数指针。回调的擦除容器有 Box 分配，但里面没有用户捕获闭包；状态值在运行时存一份，按 slot 用 `RefCell` 检查借用，不使用 Rc、Arc、unsafe 或 GC。`Access::read()` 返回有生命周期的 guard，`update()` 同步执行，释放写借用后通知所有显式读取区域。相同状态的重叠读写仍会触发借用检查；不同状态允许同时读写。`on_click_rw` 注册时拒绝对同一 ID 同时声明读和写；只读参数的写操作由编译器拒绝。它只按声明的可写权限保守失效，不检测函数是否真的改变了值。

这仍然有一个显式依赖图：state 到 region 的邻接表，没有自动追踪。静态依赖不会随 if 分支自动退订；区域重放后比较输出，没有变化就不向宿主提交 patch。watch 执行期间写状态会被运行时拒绝。`adopt` 建立显式父子关系，`unmount` 递归清理后代、订阅、handler，丢弃队列中的旧 key；不自动回收共享状态，状态由调用方 `dispose_state`。状态 handle 会校验所属 runtime；region/handler key 仍约定只用于创建它的 runtime。没有实现重放 panic 后恢复。

`RegionUi` 是只收集文字的模拟构建器，**不是完整 UiContext**。真实管线 benchmark 将每个区域的一条文字输出按 RegionId 映射到已存在的 Plico Entity，通过之前的 gated patch 钩子提交。它证明固定文本叶子可以局部更新，没有实现完整子树 reconcile、原生 `if`/`for` 增删节点、组件调用点状态恢复、自动区域所有权、异步和热重载。测试覆盖条件文字输出、静态依赖、批量去重、显式父子卸载、旧事件、状态回收后的代际检查、跨 runtime 状态误用、不同 slot 读写，以及隐式捕获/只读写入的编译失败。

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path experiments/reactivity-compare/Cargo.toml --lib
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path experiments/reactivity-compare/Cargo.toml --doc
CARGO_TARGET_DIR="$PWD/target" cargo build --release --manifest-path experiments/reactivity-compare/Cargo.toml --bin explicit_watch
PLICO_WATCH_DEMO_ONLY=1 target/release/explicit_watch
for round in 1 2 3; do
  target/release/explicit_watch > "/tmp/plico-explicit-watch-$round.csv"
done
```

2026-10-28 本机 release，三轮各自 p50 的中位数，µs。每项 5 次预热、30 次采样，交替执行顺序；计时不构建 Scene、不提交 GPU，布局与文本处理是真实 Plico 管线。80 节点预检逐帧、所有案例最终在计时外比较完整绘制命令。复用同一个 TextContext，因此字体和文字缓存共享。所有节点都在视口内；只测试纯文本叶子变化。正常 Plico 全量声明和 watch 驱动均保留同样结构的树，且两边都更新宿主状态镜像。

| N | K | immediate 总时间 | explicit watch 总时间 | immediate 前端 | watch 前端 |
|---:|---:|---:|---:|---:|---:|
| 1,000 | 1 | 682.29 | 606.68 | 59.06 | 1.09 |
| 1,000 | 100 | 810.31 | 736.81 | 64.15 | 13.26 |
| 1,000 | 500 | 1,308.46 | 1,259.16 | 73.18 | 37.01 |
| 1,000 | 1,000 | 1,920.37 | 1,910.07 | 73.40 | 72.01 |
| 10,000 | 1 | 15,159.69 | 13,823.78 | 731.68 | 2.61 |
| 10,000 | 100 | 15,624.80 | 14,652.42 | 765.28 | 73.21 |
| 10,000 | 5,000 | 26,303.44 | 26,493.44 | 858.35 | 2,196.07 |
| 10,000 | 10,000 | 37,440.36 | 38,511.31 | 871.39 | 3,381.87 |

immediate 前端是状态修改加真实 Plico 声明计时；watch 前端包含状态修改、通知、区域调度/执行、输出比较、ID 映射和节点 patch。前端计时边界不同，代表完成各自前端任务的成本，不是同一种循环的微基准。总时间可以同场景对比。各列独立取中位数，不能简单相减解释成精确的后端时间；没有固定 CPU，也没有内存常驻量与分配计数。这组结果支持“显式无捕获回调可实现，稀疏时节省前端执行”，但未证明完整动态 UI 的正确性或端到端普遍优势，也不能与前一节不同轮次的 Sycamore 数字直接排名。

## 与 Sycamore 的同轮直接对比

`watch_compare` 比较显式 watch PoC 和 Sycamore 0.9.4，两边都执行同一个 `draw` 函数，构造同样的一条文字输出、与上次输出比较，只有结果改变才交给宿主。每个状态对应一个区域/effect，初始创建与销毁不计入更新耗时。

- `graph_only`：状态修改、批量调度、回调执行、文字输出分配和比较、收集变更节点。不进入 Plico，也不包含布局；不是纯通知开销。
- `plico_cpu`：上述工作加上通过相同 Entity patch 入口更新真实 Plico 节点，再完成相同的 Adapter、文字和布局工作。两边都维护同样的宿主状态镜像，不构建计时内 Scene，不提交 GPU。

显式路径通过 RegionId 的 SecondaryMap 找到宿主节点索引；Sycamore 的 effect 闭包保存节点索引。两者是各自实际实现，并非只有调度器不同的完全同构内部存储。Sycamore 使用自动依赖跟踪，每个 effect 在此只有一个 signal。没有用它的多依赖、memo、selector 和动态图功能，也没有测试初始化、内存、动态订阅或复杂拓扑。

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path experiments/reactivity-compare/Cargo.toml --bin watch_compare
CARGO_TARGET_DIR="$PWD/target" cargo build --release --manifest-path experiments/reactivity-compare/Cargo.toml --bin watch_compare
for round in 1 2 3; do
  target/release/watch_compare > "/tmp/plico-watch-vs-sycamore-$round.csv"
done
```

2026-10-28 本机 release，以下是三轮各自 p50 的中位数。交替执行驱动顺序；graph_only 预热 10 次、采样 100 次，plico_cpu 预热 5 次、采样 30 次。逐帧在计时外验证全部输出与期望状态、变更节点集合；这也会预热输出数据。真实管线的 80 节点预检逐帧、所有案例最终比较完整绘制命令，字体上下文共享。另有测试确认一个 batch 内反复修改同一状态时，两边只提交最终值，取消回原值时不提交 patch。CPU 没有固定亲和性，下面的细小差异不能解释为稳定胜负。

| N=10,000，K | graph_only 显式 watch（µs） | graph_only Sycamore（µs） | Plico CPU 显式 watch（ms） | Plico CPU Sycamore（ms） |
|---:|---:|---:|---:|---:|
| 1 | 0.29 | 0.30 | 13.05 | 13.31 |
| 10 | 1.59 | 2.14 | 13.04 | 13.17 |
| 100 | 7.44 | 15.28 | 13.23 | 13.56 |
| 5,000 | 311.16 | 638.83 | 24.67 | 26.29 |
| 10,000 | 629.69 | 1,320.82 | 34.25 | 36.10 |

这个受限工作负载中，显式 watch 的批量前端成本较低；单节点更新很接近。接入真实后端后差距明显缩小：K=10,000 时总 CPU 时间约低 5%，不是快一倍。这说明“简单显式依赖运行时有优化空间”，没有证明它能以同样成本覆盖成熟库的全部正确性保证。显式元组和无捕获回调的作者 API 也可以包装在成熟库上，两者不必绑定。

## 嵌套区域与组件拥有的状态

新增 `nested_watch` 和 `tests/nested_regions.rs`。这是逻辑文字树实验，不是正式 Plico Engine 的子树声明接口；上述性能表记录的是加入嵌套协调之前的版本，不能视为当前嵌套实现的性能结果。

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path experiments/reactivity-compare/Cargo.toml --test nested_regions
CARGO_TARGET_DIR="$PWD/target" cargo run --release --manifest-path experiments/reactivity-compare/Cargo.toml --bin nested_watch
```

组件挂载时创建 slots，随后区域执行只解析句柄：

```rust
let component = rt.component();
let visible = rt.state_in(component, 1, || true);
let count = rt.state_in(component, 2, || 0);
let root = rt.watch_in(component, (visible,), |cx, visible| {
    cx.text("before");
    if *visible.read() {
        let count = cx.state::<i32>(2);
        cx.watch(10, (count,), |cx, count| {
            cx.text(format!("Count: {}", *count.read()));
            cx.watch(20, (count.handle(),), |cx, count| {
                cx.text(format!("Double: {}", *count.read() * 2));
            });
        });
    }
    cx.text("after");
});
```

这里的 key 都是实验手写值，尚未接 `#[component]` 的调用点注入。`RegionUi::state::<T>(key)` 只返回已经由组件初始化的 slot handle，不是完整的 `cx.state(|| init)`；因此父区域只捕获 visible，向子区域传递 count 不会让父区域订阅 count。实际读取依赖仍完全由 watch 的元组声明。组件 slots 的初始化在挂载阶段，重放阶段尚不支持新增任意局部 slot。

实现采用延迟的子区域声明：普通函数回调产生文字与子区域描述，回调结束后再协调当前父区域的孩子。子身份是“当前父实例 + key”，重放保留已有实例、替换回调和依赖，移除本轮没声明的孩子。保留文字和区域的交错顺序，支持区域输出为空、多个孩子与重排。父区域执行后，其保留的子区域也会执行，以反映参数/回调变化；这是保守的正确性规则，并没有实现输入相等时自动跳过孩子。

flush 按父子深度安排祖先先执行；父区域删除分支后，同一批次中旧孩子的任务通过代际 key 丢弃。重放中遇到已经排队的孩子不会重复执行。组件拥有状态，watch 不拥有状态：分支隐藏时移除其区域、订阅和事件，组件 slot 保留；组件卸载时先移除其根区域与后代，再释放 slots。旧事件拒绝分发，旧状态句柄不再有效。这里的移除指实际卸载分支，不是视觉上的隐藏属性。

13 项嵌套测试覆盖：

- 子状态更新不执行父区域，文字/子区域交错顺序正确。
- 父重放保留子孙 ID、组件状态和已有事件。
- 父子同时失效，不论入队顺序都只执行一次；父删除孩子时取消后代任务。
- 分支重新挂载使用新区域身份，继续读取原组件状态；旧事件不复活。
- 组件卸载使 state 析构一次，清理订阅和 handler；复用 slot 不使旧句柄有效。
- 保留的孩子更换依赖时退订旧状态；同 key 在不同父区域下互不冲突。
- keyed 孩子重排保留实例并改变输出顺序；同父重复 key 拒绝。
- 手动卸载孩子后清理输出锚点；旧版 lifetime-only adopt 关系不会在重放中丢失。
- 500 轮确定性的混合更新，每轮批量修改三次，与完整重建的预期文字序列逐项相等。
- 100 次组件挂载/卸载后，状态、区域、handler、组件和依赖边的存活计数都归零。

这证明了实验层的嵌套协调、调度和组件状态所有权可实现。尚未接入真实 Engine 的局部节点增删、布局/继承样式上下文、声明式事件重绑、列表行组件的独立状态所有权或热重载，也没有验证 panic 后继续重放；不能将逻辑文字树的正确性等同于完整 GUI 的正确性。

## 接入真实 Engine 的局部声明

`src/engine_bridge.rs` 将区域的文字/孩子描述接入实际 Plico Engine、Adapter、Taffy、文字处理和 CPU Scene。运行：

```sh
CARGO_TARGET_DIR="$PWD/target" cargo run --release --manifest-path experiments/reactivity-compare/Cargo.toml --bin engine_watch
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path experiments/reactivity-compare/Cargo.toml --test engine_regions
```

核心新增接口仍由默认关闭的 `experiment-patch` feature 控制：

- `UiContext::experiment_context()` 复制当前父 Entity、身份 scope、继承样式与复合按钮上下文，不保存借用。
- `Engine::experiment_redeclare(context, build)` 只协调此父节点的孩子；按已有 identity 复用节点，只清理本区域旧树中未声明的节点，不扫描其余 Engine 节点。
- `UiContext::experiment_keep_children()` 将一个孩子容器的后代视为不透明的保留区域，不执行或遍历其声明；它只能在局部声明内使用，不能同时声明新孩子。
- `Source::experiment_redeclare()` 合并局部声明产生的 dirty 节点、变化标记和移除 journal，再交给原 Adapter。多个区域先提交声明，最后统一准备后端。

首次挂载完整构建，此后变更只重新声明有输出变化的区域。保留的子区域通过正常 column builder 更新容器并重新捕获上下文，孩子保留；新区域先建空边界，再按父先子后的顺序声明其内容。宿主按 RegionId 保存直接内容描述，局部修改后更新这些描述，因此之后发生宿主完整重建时也会使用最新内容，而不是恢复成首次挂载的旧快照。清理描述和绑定只沿被移除的区域子树进行。

实验采用**真实 column 节点作为区域边界**，会参与布局；不是透明锚点，也不代表最终 API。文字节点和边界使用明确 key。图的回调仍产生受限文字描述，然后由桥接层调用真实 widget builder；还没有让任意 `UiContext` 控件直接运行在 watch 函数中。图的组件状态仍按上一节在挂载阶段声明。

`engine_regions` 的 9 项测试将增量路径与独立的完整声明路径比较：

- 子更新保留父节点、外部兄弟和孙区域；一次示例更新仅声明 3 个节点，而参考路径声明完整树。
- 条件分支移除、重新挂载、旧事件失效、组件卸载。
- 父子同批次更新，合并为一次后端准备。
- keyed 区域重排、插入、删除与空列表。
- 字号和颜色全局变化后刷新构建上下文，再局部更新；旧继承样式的上下文拒绝使用。
- 局部更新后，宿主主动完整声明仍得到最新结果；新建空区域也有完整重建所需的描述。
- 120 轮混合更新，每轮批量修改三次，逐轮验证真实节点的 kind、text、style、父子顺序，Taffy 位置/尺寸/裁剪，以及完整 PaintCommand 序列。
- 每次对照同时检查 Engine arena、Adapter retained 表、Taffy arena 和反向索引的节点数与实际可达树一致，避免只比较可见输出而漏掉孤立节点。
- 非法地混用“保留孩子”和“声明新孩子”会失败，随后完整声明可以恢复。

Scene 只在 CPU 编码并用于等价断言，没有窗口、GPU 渲染或像素验证。两个 Source 共享 TextContext 以消除字体资源 ID 差异，关闭动效以比较稳定布局。全局样式变化目前使用完整声明刷新上下文，不是属性级局部样式订阅。局部入口不支持 portal、自动 settle、错误回滚或 panic 后恢复；错误后需完整声明恢复。正式路径保持原来的 immediate API，不能据此宣称已支持任意组件的生产级区域重放。
