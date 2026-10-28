# 自动身份：component + view

普通组件不写 key：

```rust
use plico::{component, view, Ui};

#[component]
fn settings(ui: &mut Ui<'_>, expanded: bool, enabled: &mut bool) {
    (ui.column().gap(12)).children(|ui| {
        if expanded { ui.text("Details"); }
        ui.switch("Notifications", enabled);
        ui.button("Save").primary();
    })
}
```

plico-macros 使用 syn 解析真实 Rust AST。component 按组件命名空间与词法遍历序号生成 u64 声明 ID，并给传入 Ui 的组件/辅助函数调用建立 scope；view 展开布局、递归处理内部声明。运行时只使用生成的整数和 scope，不依赖文件路径、行号、column 或运行时分支执行次数。普通 builder 和 ui.switch/checkbox 都参与自动身份。

组件身份由 `(父 Entity, scope, local key)` 决定。条件分支不执行时，后面声明的编译期 ID 不会换号。重复组件函数/方法的不同调用表达式得到不同 scope；函数命名空间也建立边界。scope 不会制造额外布局节点。keyed 及宏 scope 使用短期 FnOnce，不保留闭包。

## 列表与显式覆盖

```rust
#[component]
fn rows(ui: &mut Ui<'_>, items: &[Item]) {
    for item in items {
        (ui.row().key(item.id).gap(8)).children(|ui| {
            ui.text(&item.label);
        })
    }
}
```

直接重复组件、无需额外 row/column 时，可用 `ui.keyed(item.id, |ui| item_component(ui, item))`。显式 builder key 覆盖生成 ID，但仍在组件 scope 内。keyed 列表重排保留身份，删除后重新挂载产生新的 Entity。

同一循环声明点重复执行却未提供业务 key，会报重复身份，避免把不同数据静默合并。重复组件位于同一个调用表达式的循环里也需要 keyed；不同静态调用表达式则自动区分。switch/checkbox 默认只需 `(label, &mut value)`，需要显式 key 时使用 switch_keyed/checkbox_keyed。

## 边界与恢复

- 完整组件优先标记 component。独立 view 内的分支、嵌套声明也有生成 ID，但多个独立根 view 的外部结构仍依赖普通 builder 的顺序 fallback；component 能统一这些根和组件调用的词法命名空间。
- 宏外普通 builder 保留按兄弟顺序的 Auto fallback，适合固定结构；不会假称任意未标记 Rust 函数都有编译期身份。
- 支持同步函数和 impl 方法，要求有命名的 `&mut UiContext` 或兼容的 `&mut Ui` 参数。传递这个 Ui 变量的普通函数/方法调用会被建立 scope；返回借用 builder 的自定义 factory 放在 children 接收者位置或双分号块的头部，由调用点直接附加 __implicit_key；其他普通 helper 在自身完成声明。含 return/break/continue/await 的调用参数应先在外面求值，宏会诊断以避免改变控制流。
- 不解析任意第三方宏里的 UI；局部独立函数需要自己标记 component。别名 view 宏不作为 component 的特殊语法识别。
- ID 不受空行、注释、文字内容变更影响；插入/删除 UI 声明、重命名组件或改变代码结构可能改变后续 ID。热更新仍是 best effort，不保证任意源码编辑保留焦点/控件临时状态。业务 App state 的 Serde 恢复独立于这些 ID。
- Scope/local ID 都是进程协议中的数值身份，不是对外持久数据格式。Engine::find 可寻找唯一的显式局部 key；多个 scoped 实例使用同名 key 时返回 None。

## 验证

core identity 测试覆盖条件插入、嵌套 view、重复函数/方法、显式覆盖、列表重排/删除/重新挂载、重复身份拒绝和新 Engine 的点击路径恢复。scripts/check-hotreload.py 验证实际动态库热更新时修改文字和插入空行后 keys 保持一致；报告在 reports/hotreload。

scripts/check-lsp.py 通过真实 rust-analyzer JSON-RPC 验证 component 内 Ui 方法、builder 方法和布局头部补全，报告在 reports/macros/lsp.json。当前是 18 个具体样本的证据，不代表任意残缺宏或任意第三方宏都能补全。使用默认 rustfmt，不引入格式配置或专用 formatter。


Context 首参的 column/row/text/text_fmt/button/button_base/svg/icon/switch/checkbox 与原 Ui 方法同样附加静态身份。text! 宏在 component 内展开到 text_fmt 后参与同一遍词法编号。普通响应组件函数仍使用 scope；借用型容器 factory 在 children 接收者位置直接附加 key，避免借用从临时闭包逸出。当前不会进行类型解析；重命名内置构造函数等任意别名还不在通用支持范围内。
