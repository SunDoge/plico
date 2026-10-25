# view 宏实验归档

`view!` 已从公开 API、应用、模板与过程宏实现中移除，`#[component]` 中的 view 语句改写与双分号迁移期检测也已删除。
当前使用 `#[component]` 提供身份与作用域，原生 `layout + |cx| { ... }` 与 `.children(...)` 完整保留。
见 [编写方式](authoring.md)。

此前尝试在一个宏中用 `layout(cx) { ... }` 省略嵌套闭包。
完整代码能编译，但编辑中新写 `text(` 时，未闭合括号可能吞掉后续语句，
导致整个宏无法展开。恢复它需要专门的 token/语句边界处理。

双分号输入本身是合法 Rust，可以复用普通 Rust 的错误恢复，维护成本较低。
相关样本与局限见 [新调用实验](experiments/fresh-call-lsp.md)。
旧宏 formatter、解析恢复代码和对应运行入口随宏移除，不再维护第二套 DSL。
