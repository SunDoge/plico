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

- 每个 Host 只有一个协程和整棵树；不支持 join、多 scope 或组件级稀疏提交。
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
