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
