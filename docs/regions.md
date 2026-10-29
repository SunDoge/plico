# 显式依赖的原生区域

`plico_runtime::regions` 是常驻 API，不再需要 Cargo feature。它保留现有 UiContext、widgets、trailing lambda 和 Engine/Adapter 管线，增加按状态 ID 失效、按区域重放的驱动。新工作默认使用 Reactive 状态模型；已有 immediate `App::show` 继续受支持，并可用于迁移对照或经 profiling 确认的高频局部路径。backend-settings 的完整路由应用已默认使用 Regions。

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

条件挂载使用 `when`，不要用会反复重建静态分支的 `watch` 模拟。条件为假时卸载内容，区域边界退出 normal flow，不会在父容器的 `gap` 中留下空白；恢复为真时重新挂载。动态业务列表使用 `each`，键为业务 `u64` ID，每项是独立 keyed component，因而可以拥有自己的 `local_state`，删除后旧句柄失效：

```rust
cx.when(visible, |visible| *visible.read(), |cx, _visible| {
    text(cx, "Mounted");
});
cx.each_gap(
    4.,
    tasks,
    |tasks| tasks.read().iter().map(|task| task.id).collect(),
    |cx, tasks, id| {
        let selected = cx.local_state(|| false);
        // 根据 id 读取任务；selected 随该 keyed item 保留。
    },
);
```

`each` 不保留列表间距；需要与 immediate 列表相同的几何时使用 `each_gap`。列表 Region 默认是一个 Scene 缓存边界，单行不默认建立独立 Fragment，避免小行在 reorder 时产生大量缓存编码。这里的 keyed 状态复用不等于每行都有独立布局或绘制缓存。

`Regions::new` 的初始化闭包同步执行，可以读取配置或创建状态；它本身不会被保存。root 和 component_keyed 的参数是组件捕获，不建立订阅。需要随状态变化重新执行的内容必须放进 watch。`Access::read` 临时借用状态，`handle` 得到可复制 ID，`update` 在修改结束后标脏显式读取者。watch/根声明期间直接修改此状态会被运行时拒绝，事件动作则在该次回调的读取借用结束后执行。这里尚未以 Read/Write 类型在编译期区分权限。

## 运行

Skia 窗口示例位于 `crates/plico-runtime/examples/regions.rs`：

```sh
cargo run -p plico-runtime --example regions --features plico-shell/skia
```

它包含计数器、条件面板、嵌套 watch、checkbox/switch、slider、TextInput 和可增删/重排的任务列表。后台仍使用正常 Source、窗口宿主和渲染器。本机 Wayland 已启动窗口；在独立 X11/Xvfb Skia 窗口中实测了计数、开关、滑块样式更新、输入/Enter 添加、反转和删除，不代表其他平台已完成视觉验收。

```sh
cargo test -p plico-runtime --test regions --test regions_controls
cargo test -p plico-runtime --doc
```

## 表单与事件 payload

受控 toggle/slider 继续使用已有 widget。短期的 Copy 值交给控件修改，`.bind` 在声明借用结束后写回 State：

```rust
use plico_runtime::regions::{BindState, OnChange};
let mut checked = *enabled.read();
checkbox(cx, "Enabled", &mut checked).bind(enabled.handle());
let mut value = *volume.read();
slider(cx, "Volume", &mut value, 0. ..=100.).bind(volume.handle());
```

字段更新或自定义行为使用显式依赖和响应值：

```rust
checkbox(cx, "Enabled", &mut checked)
    .on_change((settings.handle(),), |(settings,), checked| {
        settings.update(|settings| settings.enabled = checked);
    });
```

TextInput 绑定现有 TextBuffer，不在每次更新时重新创建 buffer。`.bind(buffer.handle())` 在内容/提交/导航变化后通知读取者，返回原 InputResponse；后面可以接 `.on_submit`：

```rust
use plico_runtime::regions::{BindState, OnSubmit};
text_input(cx, "Draft", &draft.read())
    .bind(draft.handle())
    .on_submit((draft.handle(), tasks.handle()), add_task);
```

`Access::notify()` 用于已经通过共享对象 API 修改内容后通知依赖，例如在事件中执行 `draft.read().take()` 清空输入，然后 `draft.notify()`。普通 State 替换则用 `set` 或 `update`，它们自动通知。notify/set/update 都不能直接在 watch 声明期间执行。

任务行 ID 不靠隐式闭包捕获，使用 owned payload：

```rust
use plico_runtime::regions::OnClickWith;
button(cx).text("Remove")
    .on_click_with((tasks.handle(),), task.id, |(tasks,), id| {
        tasks.update(|tasks| tasks.retain(|task| task.id != id));
    });
```

payload 必须为 `'static` 的拥有值，不能持有本轮借用。callback 仍是函数指针，参数依次为解析后的依赖元组和事件值。on_change 的值分别为 bool、f64、InputResponse；on_click_with 可以传入业务 ID 或其他拥有的数据。

事件 payload 仍可用于普通回调。keyed component 列表优先使用上述 `each`；它按业务 ID 重排存活行并保留每项 owner，不宣称列表变化只执行 K 行。列表回调需要其他状态时，应在 item 内取得对应 State 并把它列入嵌套 `watch` 的显式依赖，不能从列表依赖隐式获得其 `Access`。

## 身份、所有权与事件

Regions 根和每个 `cx.component_keyed(id, deps, callback)` 实例分别拥有状态。组件内用 `cx.local_state(|| value)` 创建带调用点身份的槽，也可用 `local_state_keyed(key, init)` 明确指定槽身份。同一业务 key 重排、resize、完整声明都复用槽，初始化函数只执行一次；组件卸载时释放其状态和所有后代 owner。删除后重新加入同一业务 key 是新实例。

```rust
cx.component_keyed(user_id, (), |cx, ()| {
    let count = cx.local_state(|| 0);
    cx.watch((count,), |cx, (count,)| {
        text!(cx, "Count: {}", *count.read());
        button(cx).text("+").on_click((count.handle(),), |(count,)| {
            count.update(|value| *value += 1);
        });
    });
});
```

状态值只存一次，State 句柄不持有 Rc/Arc，不延长 owner 生命周期。句柄包含代际 component ID，旧句柄不能访问新实例，`Regions::try_access` 对过期句柄返回 None。依赖只允许引用本组件或祖先组件的状态；兄弟组件共享值放在公共祖先。嵌套 watch 只是读取者，卸载 watch 不释放组件共享状态。组件内部局部槽按调用点匹配，不根据本轮是否调用而删除；需要独立释放的动态部分应建 component。`local_state` 不替代已有 immediate `cx.state`，后者仍是另一套机制。

watch 的默认身份来自调用位置，并结合实际父节点区分实例；当前实现使用 `track_caller` 的文件/行/列哈希，不是宏生成的编译期 ID。手写动态 Region 列表使用 `cx.watch_keyed(business_id, deps, callback)`，否则同一父节点下重复调用会冲突；需要每项独立 component 状态时优先使用 `each`。区域边界目前是真实 column，会参与布局。页面级区域可用 `watch_fill` 分配剩余 flex 空间；空容器直接写 `column(cx).grow(1.).end()`，不需要 `+ |_cx| {}`。属性绑定用 `text_empty(cx)` 建立空文字 baseline，再设置静态样式并调用 `.rx(...)`。

普通 children 闭包同步执行，可以正常借用局部变量。为了让原生 UiContext 在任意 children 层级发现当前区域，构建期间使用安全的线程局部栈，暂存构建命令与当前组件存储的 Rc；不存放 UiContext 指针或用户状态引用，不使用 unsafe。每个组件内部有一个 Rc 存储，用于在构建期间安全追加槽，句柄没有引用计数；构建临时引用在返回或退栈后释放。槽采用 elsa 的稳定地址追加容器，持有另一个槽的短期读借用时仍可创建新槽，无需框架自行编写 unsafe。

按钮使用已有 Engine 的 Entity、命中测试、键盘/语义点击分发。点击首先使所属区域执行以消费事件；on_click 将函数指针和显式参数暂存为动作，待当前读取借用结束后执行。多次写入合并，后续区域在后端提交前更新到最终值。不维护另一套长期捕获闭包或按钮事件树。

Source 的 Regions hook 在输入或状态失效时尝试局部声明；只修改状态时可以跳过组件根函数。父区域刷新孩子的回调、依赖和上下文；被移除的后代任务被丢弃。按单个区域最多八次重执行检测不收敛，嵌套层数不占用这个上限。viewport/全局上下文变化回退完整声明，状态和匹配身份保留。没有可定位的局部输入时也保守回退完整声明。

## 验证与限制

自动测试覆盖真实原生按钮/语义事件、状态变化跳过根声明、分支移除后旧事件拒绝、resize 后恢复局部更新、反复切换后的节点数量、12 层嵌套、owner 销毁释放状态，以及不同类型的三参数 watch/on_click 自动推导。编译失败测试覆盖隐式捕获。新增表单测试还覆盖 slider 数值归一化、TextBuffer 焦点/绑定保留、提交只触发一次、IME commit、secret buffer 不进入展示/IME 文本、聚焦输入卸载后的 IME 清理，以及列表 payload 删除和存活节点身份。

目前验证文字、布局、按钮，以及 toggle/slider 的绑定、普通输入和 Enter 提交、IME commit、密码展示隔离、聚焦输入卸载与重挂载。输入内容和提交计数的变化可以定位到所属区域，无需重跑组件根函数；preedit 等仅展示变化仍可能保守回退完整声明。editor、terminal、复杂 IME 场景和平台能力尚未逐项验收；backend-settings 已覆盖路由动画的无窗口及 Vello/Skia 后端回归；portal 式 modal 尚未接入区域局部声明。不要据此认为所有含可变应用借用的 immediate 用法都可直接搬入 watch。外部线程通过 StateSender 回传结果，并沿 Source 的 wake 通路唤醒 UI；此功能没有另起后台调度器。

根状态可在 Regions 构造阶段创建，局部状态可在组件回调中创建。动态依赖追踪、selector/memo、异步计算图、错误事务回滚和 panic 后恢复尚未实现。热重载采用下述 opt-in checkpoint，未标记状态重置。实验基准仍保留在 `experiments/reactivity-compare`，旧基准数据不代表新增原生接入的性能。

组件 owner 回归测试覆盖列表重排、resize 复用、卸载释放、旧句柄失效、同 key 重挂载重置，以及持有短期读借用时追加 100 个局部槽。

## 宿主异步结果

`Regions::sender(state)` 或事件参数的 `state.sender()` 得到不拥有组件的 `StateSender<T>`，T 必须 Send。宿主可把它移进已有 async executor 或线程：

```rust
button(cx).text("Load").on_click((status.handle(),), |(status,)| {
    status.set(String::from("Loading"));
    let result = status.sender();
    std::thread::spawn(move || {
        let value = String::from("Loaded"); // 宿主 I/O 的结果
        let _ = result.send(value);
    });
});
```

send 不在后台访问状态；UI 在 poll_async/下一次声明时批量应用结果，只失效相应读取者。每个状态最多保留一份待提交值，重复发送合并，邮箱从空变为非空时 wake 一次。它是 latest-value 通道，不是无损事件队列，也不替代现有 Task 的 Pending/Ready/Cancelled 状态或 executor。

组件卸载会清除待提交值；卸载后 send 返回 Err(value)。Regions 销毁后 sender 的 Weak 失效，不延长状态生命周期；同 key 新组件不接受旧 sender。成功 send 只代表入队，若随后卸载则仍会丢弃。宿主请求的取消、同一状态多个并发请求的版本先后仍由 Task/应用控制，不宣称网络副作用会自动撤销。

自动测试使用真实后台线程，验证 100 次提交合并、一次唤醒、UI 轮询前值不变、只更新订阅区域、卸载后拒绝旧结果和重挂载隔离。

## Best-effort reload checkpoint

普通 State 支持任意 `'static` 类型，不要求 Serde。明确需要恢复的槽才使用：

```rust
// 构造阶段的根状态
let count = app.persisted_state(1, || 0_i32);
// 组件回调中的局部状态
let count = cx.local_persisted_state(1, || 0_i32);
```

槽身份是「组件的 Engine 逻辑路径 + 显式槽 key」，不序列化 Entity、State ID、Rc、函数指针或订阅图。路径包含父容器/区域的身份，所以列表重排可以恢复，移动组件到不同父节点或修改调用点可能导致重置。普通 local_state 在 reload 后重置；它不要求可序列化。

`Regions::checkpoint()` 返回可序列化的 Checkpoint；每个槽通过现有 `StateSnapshot` 保存 named-field MessagePack，因此支持复合 map key 等不能直接表达为 JSON object 的类型。重新构造 Regions 后，在首次声明前调用 `restore_checkpoint(checkpoint)`，局部槽在相应组件第一次声明时恢复。类型名不符或该槽解码失败，保留该槽默认值；新增槽默认初始化，已删除槽忽略。首次完整声明后清掉未匹配记录，`restore_stats()` 报告 restored/reset/unmatched 数量。这不是 schema 自动迁移；类型重命名也可能重置。

现有开发协议适配器为 `RegionApp<P>`：实现 `RegionProgram::build() -> Regions` 提供工厂，`RegionApp<P>` 自动实现 Default、Serde 与 plico_dev::App，可作为现有模块导出的应用类型。反序列化时调用工厂重建回调/owner，再恢复标记值。直接用 `Source<RegionApp<P>>` 仍支持局部重放和宿主 wake；现有 dev 模块驱动走 App::show 的完整声明，并未声称跨 dylib 复用局部重放图。

```rust
use plico_runtime::regions::{RegionApp, RegionProgram, Regions};
struct Program;
impl RegionProgram for Program {
    fn build() -> Regions {
        Regions::new(|app| {
            let count = app.persisted_state(1, || 0_i32);
            app.root((count,), |cx, (count,)| {
                // 原生 widgets / watch / on_click
            });
        })
    }
}
type App = RegionApp<Program>;
```

自动测试经过真实 `plico_dev::render` 的点击 → checkpoint → 新会话恢复 → 再点击流程，以及局部 owner 重建、业务 key 重排、未标记值重置、类型变更、损坏单槽、增删槽和非 JSON map key。尚未在这个新适配器上执行完整的编译器/dylib 自动 reload 窗口验收。快照包含明确标记的敏感值，不自动加密，也不要标记密码缓冲等本就不该迁移的数据。

## 完整页面对照

backend-settings 的默认 `Settings` 已完整迁移到 Reactive：Overview、Activity、Preferences、Router 历史、跨路由编辑器/列表状态和有限动画均走 Regions。需要跨路由保留的 Workspace/Editor 状态由 outlet 上层 owner 持有，不保留非活跃页面组件树。`ImmediateSettings` 只用于差分和性能对照。

```sh
cargo test -p backend-settings
cargo test -p backend-settings --features vello,skia --test sidebar_motion --test skia --test vello
cargo check -p backend-settings --features egui,fltk --all-targets
```

差分测试在三个页面及 1100/520 宽度下比较语义角色、文字、值、禁用状态和叶节点几何，容差小于一个逻辑像素；动画测试覆盖事件同批 retarget、打断、reduced motion、窄屏和闲时停帧。Vello/Skia 后端回归已通过；Skia 真实窗口的路由、Details、主题与 resize，以及 FLTK 原生输入、条件挂载/卸载、路由状态和动画调度均已验收。egui 完成宿主编译检查，尚未做本次迁移的窗口验收。

[backend-settings Preferences 对照](regions-settings.md) 另外保留隔离的同步页、同控件区域版与无 GPU CPU 基准。当前小页面多数操作仍是同步版更快；边界节点的布局成本抵消了更小的声明范围，不能只看执行区域数来判断性能。默认采用 Reactive 是架构选择，不表示这些测量已出现性能反转。

静态组件可使用 `cx.component(deps, callback)` 自动生成调用点身份，动态列表继续用 component_keyed。根状态可用 `app.theme_state(state_of_theme)` 注入全局 Theme；RegionApp 已为 View/App 转发，不需要应用手写转发。主题事件同帧回退完整声明来刷新继承样式。

## 局部重放的差分验收

`plico-runtime/tests/regions_differential.rs` 让同一份区域 UI 分别走局部重放和**每次强制完整声明**。对照路径不依赖区域 dirty 判断来决定是否执行，避免两边共同漏更新。8 个固定 seed 各跑 128 批操作，每批 1–3 次变更，交错覆盖父子同时失效、依赖切换、条件卸载、业务 key 重排、输入/IME、禁用、主题、字号、resize 和迟到的宿主结果；另有固定生命周期序列。

每批比较完整声明树、语义值与顺序、布局、文字与样式、局部状态、owner 析构次数和 IME 锚点。差分包装器同时转发 Region Scene cache roots，每批将实际缓存 Scene 展开后与同一 Source、同一 TextContext 构建的全量 Scene 比较，因此该 oracle 覆盖 Fragment 复用，而不只是声明树。语义节点/快照与 hit-testing 子树缓存也经过同一 seeded 事件差分；其失效边界和强制旧路径开关见[增量执行](incremental.md#语义投影与-hit-testing-缓存2026-10-29)。独立 TextBuffer 的全局 revision 身份不同，因此仅该数字不做跨运行时相等断言，缓冲内容仍参与比较。这是无 GPU 的确定性回归，不是像素正确性或所有事件排列的证明。

检验发现并修复了同批重复点击的差异：`.clicked()` 是布尔观察，同一个控件未消费的重复 activation 现在合并，避免局部 settle 多次消费而完整声明只消费一次。已有卸载后的原生事件拒绝、局部槽释放和迟到异步结果拒绝测试继续保留。

错误契约：局部声明返回错误后显式报告，并强制下次完整声明重建边界；无法收敛返回 `SourceError::DidNotSettle`。修正错误原因后可再次声明。用户回调 panic 保留原始 payload 并向外传播，**没有事务回滚保证**；宿主捕获后应丢弃失败的 Source 并重新创建，不能继续使用可能部分修改的树。测试验证退栈清理临时声明上下文后，新 Source 可正常工作。

## Text 属性级绑定

导入 `Reactive`，可直接为普通 Text 注册显式属性依赖：

```rust
use plico::prelude::*;
use plico_runtime::regions::{Reactive, Regions};
let app = Regions::new(|app| {
    let count = app.state(|| 0_i32);
    let accent = app.state(|| Color::Accent);
    app.root((count, accent), |cx, (count, accent)| {
        text_empty(cx).font_size(24).rx(
            (count.handle(), accent.handle()),
            |node, (count, accent)| {
                node.text(format!("Count: {}", *count.read()));
                node.color(*accent.read());
            },
        );
    });
});
```

`.rx` 是 builder 结束点。单个状态直接传入并接收一个 `Access<T>`；多个状态继续使用 tuple，并接收对应的 `Access` tuple，无需为单状态保留 `(state,)` 或 `(value,)` 包装。旧 `.reactive(...)` 暂时保留为弃用别名。当前支持普通 Text 的文字、前景色、字号、字重、字体族与透明度，返回的 `TextProperties` 是受限编辑器，没有 children、事件派发或访问任意 Engine 节点的能力；富文本 spans 明确返回 InvalidRegion。普通控件、容器、布局属性尚未接入此方法，不把这个 PoC 称为通用 widget 属性系统。

每个绑定复用现有 region 的依赖调度和 component 所有权，但目标是实际 Text 的代际 Entity，不会插入布局容器。长期保存的是 State ID 元组、函数指针和静态文字/文本样式基线；不保存应用借用，也不保存捕获环境。每次运行从静态 builder 基线计算覆盖值：某个属性未调用 setter，就恢复其静态值。这保证条件属性在局部更新和完整声明中行为一致。静态文字基线可能需要一次拥有式字符串副本；绑定本身也需要订阅和调度记录，并非没有内存成本。

第一次声明立即应用属性；父区域重新声明时重新注册并应用，避免覆盖成 builder 默认值；依赖变化只执行绑定回调并更新目标，文本相等/颜色相等不产生脏标记。文字改变进入 MEASURE/LAYOUT/PAINT/SEMANTICS；字号、字重、字体族、颜色与透明度统一通过现有 Style diff 计算失效，透明度和颜色不引入测量/布局。动态属性先整体验证再修改目标节点，零字号、越界字重、NaN/越界透明度返回 InvalidStyle，不把非法值或同次文字更新写入节点。属性回调禁止写 State。节点移除时取消订阅，组件重建使用新一代身份。当前事件仍走原有 `on_click` 声明协议；属性绑定并没有将全部事件分发改为常驻回调。

`regions_properties` 回归覆盖：命名函数与异构依赖元组、初始应用、父区域/静态兄弟不执行、零节点声明、颜色不重排、文字/语义失效、多行文字改变后兄弟布局与完整声明一致、相同值无脏标记、完整声明与 resize 后属性保留、卸载与新代际、反复依赖切换后旧依赖不再唤醒、宿主异步结果只更新绑定、属性回调写入拒绝、富文本拒绝，以及条件 setter 恢复静态基线。后续增加了字体样式与透明度失效、主题 token 重解析、非法属性拒绝后恢复，以及通过开发协议重建函数/恢复状态后再次更新属性的回归；这不等同于完整 dylib 窗口热重载验收。随机差分测试也包含 text/color 属性绑定，与强制完整声明逐批比较。

原生 `regions` 示例已用此方法实现 Count、Double 和宿主请求状态文字；开启 Skia damage 的 Linux X11/llvmpipe 窗口验证了计数、异步 Loaded 唤醒、条件卸载/重建和 resize。此处不推导硬件 GPU 的性能收益；布局与绘制仍走共享的后端管线。

### 与普通 builder 共用链式属性

`color(Color)`、`font_size(u16)`、`weight(u16)`、`font_family(FontFamily)`、`opacity(f32)` 使用同一份编译期方法定义。普通 Text/控件 builder 返回自身，属性编辑器返回 `&mut Self`，两种接收方式都支持连续链式调用；共有方法的名称、参数类型和字段映射不会分别维护。校验继续统一落在 Engine 的 Style 校验边界，失效继续使用 Style diff，没有为共用方法新增运行时分发或分配。

```rust
text(cx, "Static").font_size(24).weight(700).color(Color::Accent);
text_empty(cx).rx(count, |node, count| {
    node.text(format!("Count: {}", *count.read()))
        .font_size(24).weight(700).color(Color::Accent);
});
```

这里只有共有的文本样式方法被统一；`.children()`、`.key()` 和布局/事件方法仍属于声明阶段。回归测试直接比较普通 builder 完整声明与属性局部更新的序列化节点和布局，并验证两边对零字号、越界字重、NaN 透明度返回相同的 InvalidStyle。

## 路由与页面实例

`outlet(cx, router_state, captures, render)` 将现有 Router 历史与页面组件所有权分离。控制器只订阅路由，页面接收选中的路由值和显式 State ID 捕获；相同 route key 保留 local state，切换 key 会销毁旧页并使其句柄与页内宿主 sender 失效。支持显式实例 key 的参数路由、嵌套和多个 outlet。跨页数据放在 outlet 上层；当前不提供后台 keep-alive。页面函数仍可能因导航、事件 settle 或父声明而重执行。用法、边界与窗口示例见 [Router](router.md#reactive-page-outlet)。
