# 可选 plico-async：共享 Engine 的 coroutine 组件

状态：实验 crate，不替换 `plico_dev::App::show`。原 scope/性能对照留在 core examples
作为可复现基线，新 crate 不依赖那些示例，也不使用多 Engine + 全量回放实现。

## 已实现的边界

- `plico-core/experimental-scopes`：可保留的 Layout anchor、局部声明事务、共同树布局。
  未启用该 feature 时没有新的节点标记、调度器或 API；core 不依赖 async crate。
- `plico-async`：UI 线程 Future 调度、稳定 scope 身份、跨线程 Waker 通知、取消清理。
  所有组件共享一个 Engine，没有额外绘制树或输入路由表。只执行被唤醒组件的声明。
- 普通组件内部状态使用局部变量；共享输入用显式 Input 通知，不收集读取依赖。
- MsgPack 只保存显式 checkpoint，恢复函数从头执行。不是任意 Future 序列化。
- `Runtime::take_delta` 合并所有父级/局部事务的变化和删除，不能只读取最后一轮 journal。

## 同步与 async 混排

实际入口见 `crates/plico-async/examples/mixed.rs`：

```rust
runtime.render(|cx, mounts| {
    column(cx).padding(20).gap(12).children(|cx| {
        text(cx, "Synchronous parent");
        mounts.mount(column(cx).key("counter"), counter);
        text(cx, "Synchronous footer");
    });
})?;
runtime.pump()?;
```

`counter(AsyncUi)` 是返回 `Result<()>` 的 async 函数，内部可用
`loop { ui.frame(|cx| { ... }).await?; }`。现有 widgets、`+ |cx|`、同步组件函数都可复用。
同步父级标注现有 `#[component]` 时，挂载 Layout 可沿用已有调用点 key，已验证条件
sibling 移除不改变后续组件身份；未标注时手写挂载 key，动态列表使用业务 key。
async 函数体尚无新的自动变换，不添加自动 checkpoint 或新语法宏。

父级重声明相同 anchor 时保留原 Future；未出现的 anchor 和其后代会删除，子任务
随之取消。父组件不需要常驻 borrowed UiContext：只在同步声明闭包期间借用 Engine。
`ui.mount(layout, make)` 也可以在 async 组件的 frame 中挂载子组件。

组件工厂只在初次 poll 时执行，所以再次传入不同工厂参数不会更新原捕获。
父级变化用 Input：组件初始化时 `input.subscribe(&ui)`，在 frame 闭包实际执行时
读取 `get` 或 `with`。相同值不唤醒，多次更改合并。普通局部变量不需要 Input。

## 宿主契约

Runtime 不是网络 executor；host 继续执行 IO/后台服务，交给 UI 一个可唤醒的结果 Future。
`while_pending(task, draw)` 在任务 Pending 时声明等待界面，Ready 后返回结果；整个
scope 在等待，但其他 scope 可继续交互。需要等待界面中的 Cancel 等交互时，使用
`while_pending_or(task, |cx| -> Option<Action> { ... })`，返回 Action 会 drop 请求接收
Future 并恢复顺序执行；它不撤销外部已执行的操作。裸 await 不会自动重新执行上一次视图闭包。

用 `Runtime::with_wake(notify, external_layout)` 把通知接到窗口事件循环。notify 必须
线程安全且只发通知，不能在回调中直接运行 UI。Runtime/AsyncUi 自身保持单线程。

- `render`：同步父级声明；按 `needs_parent_frame` 重声明父级。
- `queue_click` / `queue_value`：全局 Entity 输入，找到最近 retained ancestor 并唤醒。
- 原生 TextBuffer 编辑后调用 `input_changed`；无需序列化文本来回传送。
- `pump`：推进 ready 队列，默认有 10000 次 poll 的预算；不常驻 vsync 轮询。
- `set_time(FrameTime)`：只唤醒请求动画的组件；有限动画停止后不再请求帧。
- `set_theme` / `set_viewport`：唤醒组件并标记同步父级需重声明。宿主先处理父级再 pump。
- `take_delta`：一次消费跨事务合并的变化。第一次渲染也需消费。

默认 Runtime 使用 Engine 的模拟测量，便于无窗口测试；external_layout=true 则只提交
声明，真实后端负责 Taffy/Parley 等工作。尚未提供 plico-runtime/Vello/Skia 的适配器，
因此不能声称该 crate 已能直接替换现有 native App 入口。

## 取消与恢复

显式取消父 scope 或移除布局 anchor 时按子先父后 drop。Runtime 销毁也按此顺序处理。
父 Future 自然返回时，其局部值先由 Rust drop，然后宿主才能清理后代；需要活得更久的
共享资源应由 scope 外的宿主持有。取消等待不代表回滚服务器已经执行的操作。

Future 正常结束会清空 scope 内容，但保留完成记录，父级重声明同 key 不会自动重启。
要保持终态画面，应继续 frame 等待；重新挂载新实例可通过移除后再创建 anchor 完成。
完成或取消后的旧 Waker 不能唤醒新实例；Runtime 不依靠引用环保存任务。

`ui.state("count", || 0)` 返回普通值。`ui.checkpoint("count", &count)` 显式序列化
最新值，每次调用都有序列化成本，不参与更新依赖追踪。`dump_state` 保存按组件 logical path 区分的 MsgPack 字段；新 Runtime 挂载后、
首次 pump 前调用 restore_state。字段类型不兼容则初始化，未标记局部变量重置。
损坏的整体快照返回 Error，调用方可按 best effort 策略忽略并重新初始化。
尚未接真实动态库重载；旧 Future 的 poll/drop 代码必须在卸载代码前执行完毕。

## 正确性与性能边界

局部声明只修改 anchor 子树，不删除兄弟。输入属于更深子 scope 时，父级重声明或
更新不应消费/清空它。新节点的失败声明会撤回新分配，旧 committed children 顺序保留。
这不是完整状态/属性事务回滚：组件局部修改、旧节点属性和外部副作用不能自动撤销。
声明 Error 会传给宿主并结束该组件；panic 原样传播，不转换成用户可恢复错误。

保留树的 removal/commit/layout 仍有 O(N) 扫描，局部事务还需要建立子树成员集合。
密集唤醒 M 个组件时可能重复全局扫描，不能称为 O(Δ)，也不能承诺比 immediate 更快。
Scope 是实际布局容器，其尺寸和继承样式变化可能引起祖先/兄弟重排。

现有 Modal/Popover 会向全局 portal 根提交，当前局部事务明确拒绝跨 anchor 声明。
焦点恢复、全局弹窗、屏幕外 scope 的暂停保活、路由离开后的状态保留策略尚未完成。
路径形式输入也未实现局部事务收尾协议；新 Runtime 只暴露 Entity 输入。

## 验证入口

```sh
cargo run --locked -p plico-async --example mixed
cargo test --locked -p plico-async
cargo test --locked -p plico-core --features experimental-scopes --test retained_scopes
```

覆盖同步父级 + 两个 async 子组件、同批点击、稳定实例/节点、props 合并唤醒、继承禁用、
嵌套取消顺序、网络线程唤醒、晚到完成、有限动画、MessagePack 恢复/类型失败、错误与
poll 预算，以及子树删除/失败回滚/跨 Engine scope 拒绝。性能基准仍见 coroutine 实验文档；
之前多 Engine 桥接的数据不代表这个新实现的性能。
