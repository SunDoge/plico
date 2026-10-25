# Coroutine 与 immediate 写法对照

这是 experiment/frame-coroutine-ui 上的可运行作者体验比较，不是主线 API 迁移。
没有新增宏。源码在 `crates/plico-core/examples/support/coroutine_comparison.rs`。

```sh
PLICO_COROUTINE_COMPARE=1 cargo run --locked -p plico-core --example frame_coroutine
cargo test --locked -p plico-core --example frame_coroutine
```

## 比较方式

两组示例使用真实 Engine 和同样的 widgets，走相同输入/任务结果序列，在每一步比较
完整 ViewNode 快照和请求发起次数。不是只给两段伪代码，也不通过省略错误/取消路径
让 async 显得简单。宿主驱动代码与应用组件代码分开看；两种模型都需要宿主支持。

当前 immediate 示例允许用独立 CounterView/DeleteView struct 封装私有状态，不强迫
把字段平铺进 App。这更接近实际可用的组件写法，避免拿劣化的基线进行比较。

## 1. 私有计数器与父级标题

Immediate 核心：

```rust
struct CounterView { count: i32 }

fn show(&mut self, cx: &mut UiContext, title: &str) {
    text(cx, title);
    text!(cx, "Count: {}", self.count);
    if button(cx).text("Increment").clicked() {
        self.count += 1;
        cx.request_rebuild();
    }
}
```

Coroutine 核心：

```rust
async fn counter(ui: AsyncUi, title: Props<String>) {
    let mut count = 0;
    loop {
        ui.frame(|cx| {
            text(cx, title.get());
            text!(cx, "Count: {count}");
            if button(cx).text("Increment").clicked() {
                count += 1;
                cx.request_rebuild();
            }
        }).await;
    }
}
```

实际测试代码为按钮提供相同显式 key，未引入自动 key 宏。三次点击后修改父级标题，
两种视图快照一致，count 均保持 3。

结论：两者都能让事件和同步操作在一起，没有减少一次“业务回调拆分”的差别。
Coroutine 不需要组件状态 struct，但多了 loop/frame，外部变化需要 Props 通道。
Immediate 可以直接借用父级传入的 str；对于这种小组件，目前它更短、更直接。
检查点恢复不包含在这组对照中，不能把普通局部值视为自动支持恢复。

## 2. 确认 → 请求 → 失败重试 → 成功关闭

Immediate 用 Phase { Idle, Confirm, Pending, Error, Done }，show 中根据阶段声明
按钮并就地推进状态。开始/取消请求通过 Command 交给宿主执行，完成后调用 completed。
点击判断与状态变化仍在一起，但请求结果和下一步 UI 由显式状态机连接。

Coroutine 的顺序核心是：

```rust
loop {
    wait_button(&ui, "Project", "Delete", "delete").await;
    if !choice(&ui, "Delete project?", "Confirm", "confirm", "Cancel", "cancel").await {
        continue;
    }
    loop {
        let Some(result) = request_with_cancel(&ui, service.request()).await else { break; };
        if result >= 0 {
            wait_button(&ui, "Deleted", "Close", "close").await;
            break;
        }
        if !choice(&ui, "Request failed", "Retry", "retry", "Back", "back").await { break; }
    }
}
```

这段主流程更接近执行顺序，不需要手写 Phase 和 completed 分发。但辅助函数也属于
真实成本：wait_button、choice 内部仍循环 frame 并返回交互结果；可取消 loading 的
request_with_cancel 还需要 poll_fn 同时处理任务和当前帧输入。实验源码完整保留这些
实现，不能只数主流程的行数。若它们能成为成熟的通用交互组件，这份成本可以复用；
对于独有复杂面板，应用作者可能仍需自己写。

两种实现都通过以下相同序列：取消确认、确认请求、失败、重试、成功、关闭、重新请求、
等待中取消、再次请求、旧请求晚到但不影响新请求、新请求失败后 Back。请求次数与每步
视图相同。Coroutine 还验证了父 scope 卸载时丢弃子流程，随后任务完成不会重新挂载。

服务是可控的异步模拟，不是真实 HTTP；取消等待不代表撤销已经执行的外部操作。
完成和取消同轮出现时，两条路径都优先处理已就绪的完成结果，不声称覆盖其他竞争策略。

结论：顺序交互是 coroutine 当前最有价值的场景。它减少的是显式阶段与结果分发，
不是让同步声明闭包天然能 await。

## 3. Async children 候选

同一流程使用原生 async 闭包挂载在父 scope 下：

```rust
let child = async_children(&mut scopes, parent, SITE, async move |ui| {
    sequential_delete(ui, service).await;
});
```

这是真实编译运行的实验接口：同步注册有稳定身份的子 scope，Future 持有 AsyncUi
handle，不持有父级 &mut UiContext。子流程切换阶段时，父 scope 的声明次数不变。

它不是现有 LayoutBuilder::children 的 async 重载，不支持统一布局中的透明嵌入，
还沿用每 scope 独立 Engine 的限制。辅助接口每次协调该 anchor 的完整 children
列表，所以调用方必须提供专属 anchor，不能把多个独立调用当成 append。

因此两种含义必须区分：把现有同步 children 改成 async 并跨 await 持有 cx，仍未实现；
让 children 拥有独立 scope 并承载 async 流程，已验证可行。保留 loop/frame 作为安全的
临时 UI 借用边界，没有通过 TLS 裸指针或新 DSL 消掉它。

## 与 memo 的关系

计算 memo 只缓存计算结果，不改变 UI 事件写法。已有性能实验表明，对于可按输入缓存
的重计算，immediate + 计算缓存已经有效。

UI 子树 memo 则需要保存节点、布局等缓存，并定义参数、事件、主题、环境变化的失效。
当前没有实现完整的主线 UI memo API，不能承诺具体语法或把它当作已完成的替代方案。
其声明闭包可以同步执行、临时借用父状态，并不天然要求长期业务回调；代价主要是依赖
比较、缓存存储和失效规则。Coroutine scope 与它都需要保留/失效基础能力，不能靠换成
Future 省掉这些正确性问题。

当前判断：简单组件维持 immediate 很自然；顺序交互值得继续提供 async scope 能力。
还没有足够证据把整个 UI 默认改成 coroutine。下一步应该在真实页面里组合两者，并
验证共享树布局与父子输入接口，而不是先引入新语法。
