# Plico 编写方式

当前只有 immediate 声明路径：业务状态是普通 Rust 数据，事件发生时执行 show，
框架复用 retained 节点并比较属性。应用不维护 changed，不创建 Signal 或订阅。

```rust
use plico_core::prelude::*;

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Application { count: u32 }

impl plico_dev::App for Application {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).padding(24).gap(12) + |cx| {
            text!(cx, "Count: {}", self.count);
            if button(cx).text("Increment").clicked() {
                self.count += 1;
            }
        };
    }
}

#[cfg(not(target_arch = "wasm32"))]
plico_dev::export_app!(Application);
```

使用 `plico new PATH` 生成普通 main.rs + lib.rs 项目；desktop 是可选依赖，
`cargo build --lib --no-default-features` 生成供通用宿主加载的模块。
静态入口使用 `plico_backend::run(Application::default(), title)`。

布局使用 Context 首参的独立函数；自定义组件也使用普通 Rust 函数。
`#[component]` 注入调用点身份，动态列表仍使用业务 key。容器通过 `+ |cx| { ... }`
或底层 `.children(...)` 嵌套。已有字符串借用，动态文字优先使用 text!。
时间由 Clock/FrameTime 注入，不在组件中读取系统 Instant。

异步任务留在宿主服务中，结果通过快照或消息传回 UI；任务执行不依赖重复声明。
见 [宿主服务](host-services.md)、[生命周期](lifecycle.md)、[开发 CLI](cli.md)、
[自动身份](identity.md)、[图标](icons.md)。
