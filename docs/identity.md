# 自动身份：`#[component]`

普通组件不需要为静态节点手写 key：

```rust
use plico::prelude::*;

#[component]
#[rustfmt::skip]
fn settings(cx: &mut UiContext, expanded: bool, enabled: &mut bool) {
    column(cx).gap(12) + |cx| {
        if expanded {
            text(cx, "Details");
        }
        switch(cx, "Notifications", enabled);
        button(cx).text("Save").primary();
    };
}
```

`plico-macros` 使用 `syn` 处理 Rust AST。`#[component]` 按组件命名空间和词法声明顺序生成数值身份，并为传入 `UiContext` 的组件调用建立 scope。运行时身份不依赖文件路径、行号、列号或分支执行次数。

组件身份由 `(父 Entity, scope, local key)` 决定。条件分支暂时不执行时，后续声明不会因为运行时计数换号；同一组件在不同静态调用点也有不同 scope。scope 不制造布局节点。

## 动态列表

动态列表必须使用业务 ID：

```rust
#[component]
fn rows(cx: &mut UiContext, items: &[Item]) {
    for item in items {
        cx.keyed(item.id, |cx| {
            row(cx).gap(8) + |cx| {
                text(cx, &item.label);
            };
        });
    }
}
```

也可以对 builder 使用 `.key(item.id)`。显式 key 覆盖生成的局部 ID，但仍处于组件 scope 中。列表重排会保留身份；删除后再次挂载会得到新 Entity。同一声明点重复执行却没有业务 key 时会报告重复身份，避免把不同数据静默合并。

## 边界

- `#[component]` 支持同步函数和 impl 方法，要求存在命名的 `&mut UiContext` 参数。
- 宏处理普通 Rust 调用、原生 `.children(...)` 和 `+ |cx| { ... }` 中的声明；不解析任意第三方宏内部的 UI。
- 自定义借用型容器 factory 在 children 接收者位置由调用点附加身份；普通组件函数自身使用 `#[component]`。
- 静态结构在宏外仍可使用兄弟序号的 `Auto` fallback。动态列表不要依赖该 fallback。
- 空白、注释和文字内容变更不影响 ID；插入声明、重命名组件或改变代码结构可能影响后续 ID。持久业务数据必须使用自己的 storage key。
- scope/local ID 是进程内声明身份，不是持久化格式。

`view!`、`Ui` 别名和双分号 children 已移除。当前布局只使用普通 Rust builder、`+ |cx| { ... }` 与底层 `.children(...)`。

## 验证

核心测试覆盖条件声明、嵌套组件、函数与方法调用、显式覆盖、列表重排/删除/重新挂载和重复身份拒绝。`plico fmt` 只负责 `#[component] + #[rustfmt::skip]` 的排版，不参与编译或身份语义；`plico fmt --check` 只检查格式。
