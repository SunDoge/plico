# Plico 编写方式

Plico 只保留一条应用主路径：普通 Rust 状态与 immediate `App::show`。应用每次声明
期望的界面，Engine 在后端保留节点、文字、布局与绘制缓存；应用不维护 `changed`，也不
需要另一套 reactive 状态图。

```rust
use plico::prelude::*;

#[derive(Default)]
pub struct Application { count: u32 }

impl plico_dev::App for Application {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext<'_>) {
        column(cx).padding(24).gap(12) + |cx| {
            text!(cx, "Count: {}", self.count);
            if button(cx).text("Increment").primary().clicked() {
                self.count += 1;
            }
        };
    }
}
```

使用 `plico new PATH` 生成普通 `main.rs + lib.rs` 项目。静态桌面入口使用
`plico_desktop::run(Application::default(), title)`；需要持久化的数据由应用或
Engine 的显式持久状态保存，不依赖动态热重载 ABI。

布局使用 `UiContext` 首参的独立函数；自定义组件也优先使用普通 Rust 函数。
`#[component]` 只注入组件和调用点身份。容器通过 `+ |cx| { ... }` 或底层
`.children(...)` 嵌套。静态节点不手写 key，动态列表使用业务 ID。已有字符串直接
借用，动态文字优先使用 `text!`。

控件直接报告常用交互，和 MyGO 的 Element API 保持同一使用习惯：

```rust
if button(cx).text("Save").clicked() { save(); }
let input = text_input(cx, "Name", &name).response();
if input.changed { validate(); }
```

时间由 `Clock`/`FrameTime` 注入，不在组件中读取系统 `Instant`。异步任务留在宿主
服务中，结果通过只读快照和命令传回 UI；宿主服务不持有 UI 状态引用。

见 [宿主服务](host-services.md)、[生命周期](lifecycle.md)、[开发 CLI](cli.md)、
[自动身份](identity.md)、[图标](icons.md)。
