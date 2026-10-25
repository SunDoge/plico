# Frame-yielding coroutine 实验

这是独立、无窗口的 API 实验，不改变 `App::show`，不重新引入 reactive 图。
实现位于 `crates/plico-core/examples/frame_coroutine.rs`，使用真实 Engine 和 widgets。

```sh
cargo run --locked -p plico-core --example frame_coroutine
cargo test --locked -p plico-core --example frame_coroutine
```

## 验证的模型

一个 async 函数拥有一个 Engine。普通 `i32` 局部变量存活在 Future 中，声明闭包
同步借用局部状态；整个实验没有 unsafe、TLS context 或新增依赖。`Rc<RefCell<_>>`
只用于宿主与协程共享单线程 Engine 和检查点，不包装每个局部变量。

`frame` 在本 pass 内同步声明并立即返回结果。协程可以立即处理点击，然后下一次
`frame` 在相同 pass 中返回 Pending，防止重复声明。宿主只有收到输入、显式失效或
任务 Waker 时才推进 pass；这不是通用 executor，也不是按 vsync 不停 poll。

计数器展示 `let mut count: i32 = ui.state("count", || 0)`：state 返回普通值，
不追踪其修改。每次处理完交互显式 checkpoint，修改影响已声明的文字时调用现有 `cx.request_rebuild()`，桥接器将 VIEW 标记转换成宿主唤醒。
这里没有通过语法自动发现依赖。

恢复实验序列化检查点、丢弃旧 Future，再从函数开头创建新 Future。count 恢复，
未保存的 transient 重置，类型不兼容时初始化。这不是实际动态库 reload 验证，
也不恢复 await 位置。实验检查点使用已有 dev-dependency serde_json；正式协议仍由
plico-dev 管理，未修改其格式。

计数器的点击判断和 `count += 1` 已放在同一个同步声明闭包里，不再返回点击事件到外层。
新增表单验证局部 enabled、现有 TextBuffer 输入、就地 Save 处理、恢复选定字段而重置
临时反馈。普通局部值不需要 wrapper，但输入仍复用已有 TextBuffer 的编辑与选择存储。
同步处理能直接就地执行；真正的 await 仍不能放进同步 children 闭包，这个边界未解决。

另一个流程验证确认 → 等待外部任务 → 结果显示。任务真正保存并触发 Waker，
不依赖 poll 次数模拟完成。等待时宿主可以停帧，任务完成后唤醒。取消会 drop Future
并提交空视图，晚到的任务完成通知不会重新挂载 UI。

## 当前边界

- 每个 Host 只有一个协程和整棵树；scope 实验组合多个 Host，仍不支持共享树局部提交或 join。
- 确认和 loading 替换整个视图，不是覆盖在仍可交互的背景之上的 modal。
- Engine 的声明、匹配和提交扫描仍然存在，没有属性级 reactive 性能收益。
- 未接窗口事件循环、动画时钟、宿主服务及真实动态库；闭包渲染错误目前直接 panic。
- 普通局部状态必须主动 checkpoint/invalidate。稳定 ID 不能自动观察任意变量修改。
- 有返回值的有限协程会立即卸载；若要展示终态，必须继续维持终态声明。
- pump 最多八次稳定化；一次 poll 内阻塞、死循环或重 CPU 仍会阻塞宿主。
- state key 仅在单 Host 内唯一；尚无组件实例命名空间、版本或重复 key 诊断。

## 结论

普通局部状态与同步即时声明可以在原生 Rust async 中组合；无需 signal 包装或
不安全的栈 context 指针。收益主要是状态与流程表达，不是省掉声明执行。
代价包括每个长期流程的 Future 存储、Engine 共享间接访问、显式失效与检查点。
是否引入共享树 scope、自动 checkpoint 宏和宿主适配，仍需后续实验。


## 父子 scope 实验

`examples/support/coroutine_scopes.rs` 复用相同 Host，使用 generational Arena 保存
父子所有权。每个 scope 有独立 Engine、Future、Waker 和检查点空间。输入目前由测试
显式指定 scope，节点身份必须理解为 `(scope id, engine-local entity)`；不同 Engine
的 Entity 不能单独用作全局输入 ID。

已验证：

- counter 与等待任务的流程独立推进；等待期间 counter 可点击，任务完成不重新声明兄弟。
- 两个 counter 使用相同 state key，不共享状态；只重建一个 Future 不重新声明另一个。
- 取消 scope 沿所有权树按子先父后清理，保留无关兄弟；父任务自然完成后也移除后代。
- scope 删除后，旧事件被拒绝；旧任务的晚到 Waker 不会唤醒复用 arena 槽位的新 scope。

这只是调度和所有权实验，不是多个组件已经可以拼成统一窗口。各 Engine 之间没有
共同的 Taffy 布局、焦点、portal 或事件命中系统；也没有共享树、共享缓存、合成输出。
调度使用 O(scope 数量) 的列表扫描并创建临时 ID 列表，但只 poll 被唤醒的任务。
因此可以证明跳过兄弟业务声明，不能证明整体 O(Δ) 或减少内存；每个 scope 一个 Engine
反而有固定内存开销。不能把它直接推广为每个 widget 一个 Engine。

外部取消可以保证子先父后 Drop；父 Future 自然返回时，它内部局部值先由 Rust
析构，宿主观察到 Ready 后才能清理后代。若父资源必须活到所有子任务结束，需要把
这类资源放在宿主 scope 中，或先显式等待/取消子任务，不能依赖 async 局部析构顺序。

下一步若继续推进，关键是共享 Engine 的局部声明事务、统一布局和组件挂载 API，
而不是增加 DSL。当前 mount 在宿主侧完成，还没有在 async 父函数内借用并挂载子组件。

## 调用点身份与父级协调

新增 `reconcile(parent, children)` 验证静态调用点 ID 的复用。每个 child spec 包含
调用点 site、可选业务 key 和仅初次挂载执行的 Future 工厂。父级提供完整 child 列表，
协调器按 `(parent instance, site, business key)` 复用实例，更新顺序，并删除缺席的子树。
site 手写为常量或调用现有 `macro_support::site_id`，没有新增宏。

验证结果：列表重排不重新创建或 poll 子组件；局部 count 保留；条件 sibling 消失不会
改变其他实例；重新挂载已移除项会创建新实例并重置局部状态。同样的 site 在不同父级
隔离，重复身份在创建/删除前拒绝。该协调器在宿主侧执行，不是 async 父函数中的组件
挂载 API，也不具备工厂 panic 时的事务回滚。

发现并明确测试了一个限制：复用 Future 会保留初次捕获的参数。父级再次提供不同参数
的工厂不会更新旧 Future。这意味着 stable ID 只能解决身份，不能代替 props 协议。

实验新增显式 `Props<T>` 输入槽：更改值时唤醒目标组件，相同值不唤醒，多次更改合并为
最新值。验证了标题更新保留局部 count、父组件不重声明、卸载后更新无效。这里使用
Rc/RefCell 共享跨组件输入，不包装局部 count，也没有自动依赖收集。正式挂载接口尚未
把 props 更新和 reconcile 集成；目前由宿主显式持有并更新输入槽。

读取 props 的时机也重要：必须在 frame 声明闭包执行时读取。若在构造这个闭包之前
复制 props，然后停在 await，恢复时运行的是旧捕获。这是该模型真实存在的 API 约束，
未来组件 API 需要让“当前输入”自然出现在声明闭包内，不能单凭 async 解决。

因此下一阶段仍不能推广为默认模型：共享树提交、跨 scope 布局、组件输入协议和父级
挂载语义仍需要共同验证。当前结果支持“普通局部状态 + 明确的组件输入边界”，不支持
“所有值都可以直接借用且自动更新”的结论。

## 单 Engine 合成与共享布局验证

后续 `examples/support/coroutine_composite.rs` 增加公开 `element_with_grid` 回放层：
每个 scope 仍保有自己的 Engine，但其保留节点被重放到一个共同 Engine 中。共同树
用 scope 实例和局部 Entity 构成命名空间，scope 的内容容器与子 scope 分开命名，
避免同名按钮或局部 arena handle 冲突。这不是共享 Engine 的原地子树事务。

已验证：

- 两个同名 Increment 按钮获得不同全局 Entity；全局点击通过公共 replay 返回值送往
  对应 scope 的本地节点，再由协程处理；兄弟业务声明次数不变。
- 列表重排保持全局控件 Entity；删除 scope 后拒绝旧点击，并移除共同树的旧节点。
- 子 scope 高度从 30 增至 100，共同根高度增加 70；兄弟 Future 不执行，但祖先布局
  正常失效。使用的是核心 Engine 模拟布局，不是 Taffy、实际坐标命中或窗口截图验证。
- 共享树 TextInput 绑定到现有本地 TextBuffer，编辑后 Save 可读到新值；删除 scope
  会移除共享 Engine 的输入绑定，不需要为传输编辑内容新增序列化通道。
- 100 个 counter 中点击一个，该组件执行两次声明（消费点击 + 重建显示），其余
  99 个跳过业务声明。但共同树回放仍访问 701 个节点；稳定后的布局运行次数为 0。

这个结果验证了“组件业务代码稀疏执行 + 公共协调层全量重放”可以组合，而不是验证
整条更新链路已经变成 O(Δ)。点击目前还需要事件回放和更新后回放两个阶段；全量回放、
临时路由表和多 Engine 存储都可能抵消简单组件的收益，尚无 CPU/内存对照结论。

原型只支持列/行、普通文本、按钮、开关、复选框和输入节点；遇到未覆盖节点、富文本、
shader 或 numeric payload 会显式拒绝。没有宣称完整 widget、portal、IME、焦点、
全局主题/viewport 失效、动效或真实后端兼容。新增 scope 包装容器也会影响 flex/grid
语义，不能直接视为透明组件边界。

当前最值得继续比较的两条路线是：保留现有轻量 replay 作为简单正确的基线，或者
为 Engine 增加真正的局部声明事务。应先测全链路成本，再决定是否值得引入后者。

## 仿真 benchmark（2026-10-25）

复现：

```sh
python3 scripts/benchmark-frame-coroutine.py
# 可选环境变量：PLICO_BENCH_RUNS=3、PLICO_BENCH_CPU=0
```

脚本编译 release 后直接运行二进制，默认三轮；Linux 固定到同一个允许使用的 CPU。
每组预热 5 次、计时 21 次，冷挂载单独测 7 次。CSV、汇总与环境记录写到 reports，
不参与提交。可直接运行一轮：

```sh
PLICO_COROUTINE_BENCH=1 cargo run --locked --release -p plico-core --example frame_coroutine
```

比较四条路径：普通 immediate、与回放相同包装容器的 immediate、当前 coroutine +
全量共享回放，以及普通 immediate + 按输入缓存纯计算结果。100/1000 个组件，
0/64/1024/8192 轮合成 CPU 计算，场景为强制稳定重放、单点数据更新、10% 批量更新、
全量更新、单点真实点击和冷挂载。更新输入与协调工作在计时内，统计声明次数的观测代码
在计时外。点击包括消费事件与稳定化，三条 immediate 路径重声明两遍整树，coroutine
路径执行两遍目标组件和两遍共享回放。

仿真使用相同的计数文字、推导文字与按钮。语义测试检查四条路径在稀疏/批量/全量更新
及点击后输出相同文字。模拟计算是 black_box 包裹的整数运算，不是对真实应用负载的
估计；不能把 rounds 当成通用交叉点。实验没有测 reactive/signal 库、GPU、窗口、
Taffy/Parley 或序列化检查点，不能据此声称优于 signal。稳定重放是显式强制调用，
不代表 immediate 在无事件时仍需空转。

本机：Intel Core Ultra 9 285K，CPU affinity 0，rustc 1.99.1，release。
下表为三轮各自中位数的中位数，单位 ms，1000 个组件：

| 合成计算 / 更新 | immediate | 同形状 immediate | coroutine 回放 | immediate + 计算缓存 |
|---|---:|---:|---:|---:|
| 0 / 单点数据 | 0.454 | 0.666 | 2.167 | 0.446 |
| 0 / 全量数据 | 0.480 | 0.680 | 2.712 | 0.469 |
| 0 / 单点点击 | 0.920 | 1.348 | 4.268 | 0.906 |
| 1024 / 单点数据 | 1.357 | 1.583 | 2.143 | 0.448 |
| 8192 / 单点数据 | 7.851 | 8.083 | 2.164 | 0.443 |
| 8192 / 全量数据 | 7.939 | 8.078 | 10.371 | 7.868 |
| 8192 / 单点点击 | 15.804 | 16.220 | 4.302 | 0.891 |

100 个轻组件单点更新：普通 immediate 约 39.7 µs，coroutine 约 97.4 µs。
1000 个轻组件单点更新时，普通 immediate 执行 1000 个组件、访问 5001 个节点；
coroutine 只执行 1 个组件，但共享层仍访问 7001 个节点。给 immediate 加相同包装
容器后仍约为 coroutine 耗时的 31%，说明差异不能只归因于额外布局节点。

预热后的数据更新：两种直接 immediate 与计算缓存路径分配 0 次；coroutine 每次
分配 1 次、申请 8000 bytes，来自 scope 调度的临时 ID 数组。单点点击还包含回放
事件暂存；首次点击会扩展输入队列，因此平均分配次数可以是小数，并非每次都分配。
1000 个组件冷挂载：普通 immediate 约 1.268 ms、6075 次分配、累计申请 18.25 MB；
coroutine 约 8.782 ms、33097 次、34.65 MB。累计申请量包含扩容，既不是常驻内存，
也不是峰值 RSS。计时包含分配计数器的原子操作开销，尤其冷挂载不应视为无探针数据。

结论：当前原型没有普遍性能优势。重声明中的纯计算昂贵且更新稀疏时，跳过业务确实
有效；但该例使用普通输入缓存即可取得更好的结果，不需要 async 组件。当前共享回放
还有全树协调、路由表重建、线性查找与多 Engine 固定存储，不能将这些代价归因于
Rust Future 本身，也不能假设换成真正局部提交后必然划算。

这组数据只支持对当前桥接原型的评价，不能把耗时归因为 coroutine；后续隔离实验见下节。
开发体验上的候选收益仍是局部变量状态、顺序流程与明确的 scope 生命周期。
继续推进前，应比较真实应用中的开发体验与简单缓存方案，或者先改进共享回放的
路由/提交路径后复测；不据此替换现有 immediate 主线。


## 归因修正：隔离 Future 与桥接成本

前一组是端到端原型比较，不是 Future 开销隔离实验。仅凭它得出“coroutine 模型没有
性能优势”的结论不成立。补充的隔离套件保留原型结果，并把问题分为三层：

```sh
PLICO_BENCH_SUITE=isolation python3 scripts/benchmark-frame-coroutine.py
```

同样 release、CPU 0、三轮，每组 21 个样本。新增 87 组对照，结果放在 reports 的
isolation 子目录。没有新增宏或修改主线 Engine。

### 同一 Engine 管线，只替换驱动方式

四条路径使用同一个 `draw_tree` 函数、相同节点/布局、相同 Rc/RefCell 输入，以及
一个活动 Engine；没有多 scope Engine、共享重放或路由表。比较直接 Engine::frame、
同步 AsyncUi::draw、具象 Future 驱动、dyn Future 驱动。后两者是一个 Future 声明
整棵树，刻意不做稀疏跳过，以隔离驱动开销；不是完整组件调度器的性能估计。

预热后更新输入和执行驱动在计时内，Future 构造、Box 和输入初始化不计入。
语义测试验证同步与 Future 路径的完整 ViewNode 快照和 FrameStats 一致，并验证
同 pass 重复 poll 不会重复声明。四条驱动路径每次更新均无分配。

1000 个组件的三轮中位数汇总，单位 µs：

| 工作 / 更新 | 直接 frame | 同步桥接 | 具象 Future | dyn Future |
|---|---:|---:|---:|---:|
| 0 / 单点数据 | 453.02 | 439.92 | 437.80 | 439.71 |
| 0 / 全量数据 | 474.51 | 461.58 | 458.80 | 463.53 |
| 8192 / 单点数据 | 7757.12 | 7754.92 | 7773.01 | 7786.27 |
| 8192 / 全量数据 | 7805.99 | 7776.33 | 7770.83 | 7792.58 |

这些结果没有原型比较中的数倍差距，驱动差异小于整个声明管线成本。轻量单点的各轮
中位数范围：直接 437.58–459.31 µs、具象 Future 437.46–444.70 µs，相互重叠。
不能据微小差异声称 Future 更快，也不能把整帧耗时相减当作精确 poll 成本。

### 原型分层计时

把原有 `Composite::commit` 等价拆为 replay 和 rebuild_routes，原 commit 仍依次调用
二者，不改变协议。每次更新按相同顺序测 local pump、shared replay、route rebuild，
前置输入更新和观测计数放在相应计时之外。阶段计时会扰动缓存、含计时器开销，
各阶段中位数不应精确相加来替代端到端结果。

1000 scope、单点更新，单位 µs：

| 每组件计算 | local pump | shared replay | route rebuild |
|---|---:|---:|---:|
| 0 | 9.45 | 976.93 | 1160.99 |
| 8192 | 16.89 | 982.76 | 1164.62 |

主要代价明确在桥接层：共享回放遍历整树，路由重建又遍历和查找。路由重建还包含
大量 sibling 线性 find，在平铺的大量 scope 场景中会放大查找成本。local pump 包含
scope 列表扫描与目标组件声明，不是纯 poll，但已明显小于桥接成本。

### 纯状态机微基准

再移除 Engine 和 UI，比较同样 Rc<Cell<u64>> 计数操作的同步循环与编译器 async
状态机。Future 先 poll 一次进入 Pending，随后每次 poll 恢复、增加一次计数、再挂起。
具象/动态 Future 每样本 poll 100000 次，循环包含 black_box，最后断言次数一致。
没有 executor、唤醒、IO、跨线程或每次 poll 的分配。

本机每操作约为同步 0.356 ns、具象 Future 0.706 ns、动态 Future 1.052 ns。这是高度
可优化的小循环，只用来说明此状态机没有毫秒级开销，绝不是通用 Future/网络任务的
poll 延迟。真实组件还会有输入读取、scope 查找、声明和失效处理。

修正后的结论：当前原型慢的主要原因是多 Engine 到共同树的桥接，而非已证明的 Rust
coroutine 性能问题。局部 Future 驱动本身没有显示出阻止该模型的开销；若要验证其
性能潜力，下一步应减少共享回放/路由成本，或实验共享 Engine 原地子树提交，再做
同工作量对照。仍未与 signal 库比较，也没有证明最终模型必然优于 immediate。
