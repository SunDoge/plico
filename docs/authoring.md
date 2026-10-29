# Plico 编写方式

新应用默认使用 Regions Reactive：组件拥有普通 Rust 状态，事件通过显式依赖写回，
只有读取该状态的区域需要重放。应用不维护 changed，也不需要额外宏。已有 immediate
`App::show` 仍作为底层 API、迁移对照和经 profiling 确认的局部路径保留。

```rust
use plico::prelude::*;
use plico_runtime::regions::{OnClick, Reactive, RegionApp, RegionProgram, Regions, State};

#[component]
#[rustfmt::skip]
fn application(cx: &mut UiContext, count: State<u32>) {
    column(cx).padding(24).gap(12) + |cx| {
        text_empty(cx).rx(count, |node, count| {
            node.text(format!("Count: {}", *count.read()));
        });
        button(cx).text("Increment").on_click(count, |count| {
            count.update(|value| *value += 1);
        });
    };
}

pub struct Program;

impl RegionProgram for Program {
    fn build() -> Regions {
        Regions::new(|app| {
            let count = app.persisted_state(1, || 0_u32);
            app.root((count,), |cx, (count,)| application(cx, count.handle()));
        })
    }
}

pub type Application = RegionApp<Program>;

#[cfg(not(target_arch = "wasm32"))]
plico_dev::export_app!(Application);
```

使用 `plico new PATH` 生成普通 main.rs + lib.rs 项目；desktop 是可选依赖，
`cargo build --lib --no-default-features` 生成供通用宿主加载的模块。
静态入口使用 `plico_desktop::run(Application::default(), title)`；`persisted_state`
标记需要跨开发热重载保留的值，普通 `state` 和 `local_state` 在模块重建后重置。

布局使用 Context 首参的独立函数；自定义组件也使用普通 Rust 函数。静态区域用
`watch`/`component`，条件内容用 `when`，动态业务列表用 `each`。immediate 组件中的
`#[component]` 注入调用点身份。容器通过 `+ |cx| { ... }`
或底层 `.children(...)` 嵌套。已有字符串借用，动态文字优先使用 text!。
时间由 Clock/FrameTime 注入，不在组件中读取系统 Instant。

异步任务留在宿主服务中，结果通过快照或消息传回 UI；任务执行不依赖重复声明。
见 [宿主服务](host-services.md)、[生命周期](lifecycle.md)、[开发 CLI](cli.md)、
[自动身份](identity.md)、[图标](icons.md)。
