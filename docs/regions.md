# 显式依赖的原生区域实验

`plico-runtime/regions` 是默认关闭的实验功能。它保留现有 UiContext、widgets、trailing lambda 和 Engine/Adapter 管线，增加按状态 ID 失效、按区域重放的可选驱动。默认应用继续使用 immediate `App::show`；这不是新的默认框架模型。

## 写法

```rust
use plico::prelude::*;
use plico_runtime::regions::{Regions, WatchUi, OnClick};

let app = Regions::new(|app| {
    let count = app.state(|| 0_i32);
    app.root((count,), |cx, (count,)| {
        column(cx).gap(12).children(|cx| {
            cx.watch((count.handle(),), |cx, (count,)| {
                text!(cx, "Count {}", *count.read());
            });
            button(cx).text("Increment").on_click((count.handle(),), |(count,)| {
                count.update(|value| *value += 1);
            });
        });
    });
});
let source = plico_runtime::Source::new(app);
```

root、watch 与 on_click 都通过依赖元组选择具体函数指针类型，参数类型自动推导。watch 为 `|cx, (a, b)|`，事件为 `|(a, b)|`；空元组分别写 `|cx, ()|` 和 `|()|`。目前支持 0–6 个状态参数，不要求 proc macro。具名函数也可直接传入，不用类型转换。元组解析用同一套 trait，不为每种参数数量维护不同的回调对象。未列出的隐式捕获会编译失败。

`Regions::new` 的初始化闭包同步执行，可以读取配置或创建状态；它本身不会被保存。root 的参数是组件捕获，不建立订阅。需要随状态变化重新执行的内容必须放进 watch。`Access::read` 临时借用状态，`handle` 得到可复制 ID，`update` 在修改结束后标脏显式读取者。watch/根声明期间直接修改此状态会被运行时拒绝，事件动作则在该次回调的读取借用结束后执行。这里尚未以 Read/Write 类型在编译期区分权限。

## 运行

Skia 窗口示例位于 `crates/plico-runtime/examples/regions.rs`：

```sh
cargo run -p plico-runtime --example regions --features regions,plico-shell/skia
```

它包含计数器、按钮、条件面板和嵌套 watch。后台仍使用正常 Source、窗口宿主和渲染器。示例经过编译验证，控件事件通过 headless 测试验证；不把编译成功当成跨平台窗口视觉验收。

```sh
cargo test -p plico-runtime --features regions --test regions
cargo test -p plico-runtime --features regions --doc
```

## 身份、所有权与事件

每个 Regions 实例是一个状态 owner。状态值只存一次，句柄不持有 Rc/Arc，不延长 owner 生命周期；owner 销毁即释放状态。嵌套 watch 只是读取者，卸载 watch 不释放组件共享状态。尚未提供嵌套组件 owner 或列表行的独立状态生命周期。

watch 的默认身份来自调用位置，并结合实际父节点区分实例；当前实现使用 `track_caller` 的文件/行/列哈希，不是宏生成的编译期 ID。动态列表必须用 `cx.watch_keyed(business_id, deps, callback)`，否则同一父节点下重复调用会冲突。区域边界目前是真实 column，会参与布局。

普通 children 闭包同步执行，可以正常借用局部变量。为了让原生 UiContext 在任意 children 层级发现当前区域，构建期间使用安全的线程局部栈，暂存仅含构建命令的 Rc；不存放 UiContext 指针或用户状态引用，不使用 unsafe。这个临时元数据 Rc 与状态所有权无关，回调返回或退栈后释放。

按钮使用已有 Engine 的 Entity、命中测试、键盘/语义点击分发。点击首先使所属区域执行以消费事件；on_click 将函数指针和显式参数暂存为动作，待当前读取借用结束后执行。多次写入合并，后续区域在后端提交前更新到最终值。不维护另一套长期捕获闭包或按钮事件树。

Source 的可选 hook 在输入或状态失效时尝试局部声明；只修改状态时可以跳过组件根函数。父区域刷新孩子的回调、依赖和上下文；被移除的后代任务被丢弃。按单个区域最多八次重执行检测不收敛，嵌套层数不占用这个上限。viewport/全局上下文变化回退完整声明，状态和匹配身份保留。没有可定位的局部输入时也保守回退完整声明。

## 验证与限制

自动测试覆盖真实原生按钮/语义事件、状态变化跳过根声明、分支移除后旧事件拒绝、resize 后恢复局部更新、反复切换后的节点数量、12 层嵌套、owner 销毁释放状态，以及不同类型的三参数 watch/on_click 自动推导。编译失败测试覆盖隐式捕获。

当前首先验证文字、布局和按钮的完整通路。可在回调中调用原生 widget，但没有逐项验收 text input、IME、slider、editor、terminal、动画和平台能力。不要据此认为所有含可变应用借用的 immediate 用法都可直接搬入 watch。外部状态修改仍需要宿主事件唤醒；此功能没有另起后台调度器。

状态创建目前在组件构造阶段，不是在任意重放函数中调用 `cx.state`。现有 immediate local state 与此处的 State 是不同机制。动态依赖追踪、selector/memo、异步计算图、热重载状态迁移、错误事务回滚和 panic 后恢复尚未实现。实验基准仍保留在 `experiments/reactivity-compare`，旧基准数据不代表新增原生接入的性能。
