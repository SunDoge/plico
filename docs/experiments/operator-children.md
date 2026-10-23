# 原生 Rust `+ closure` children 实验

2026-10-21 验证后采用。已迁移 workspace 中 105 处双分号布局、生成模板和当前文档，旧语法不再支持。

```rust
#[component]
#[rustfmt::skip]
fn settings(cx: &mut UiContext) {
    column(cx).gap(16).padding(20) + |cx| {
        text(cx, "Settings").font_size(24);
        row(cx).gap(8) + |inner| {
            button(inner).text("Save").primary();
            button(inner).text("Cancel").ghost();
        };
    };
}
```

`Layout` 实现 `Add<F>`，F 为 FnOnce(&mut UiContext) -> R，内部直接调用 consuming children，返回 R。与已有 children 一致，布局返回闭包结果；不是额外增加一层 Response。builder 被消费，外层 context 借用到闭包执行结束即释放，随后可继续声明兄弟节点。`row/column`、Button/ButtonBase、Modal/Popover、Tooltip 以及返回 Layout 的自定义工厂可用；gallery 的自定义 Panel 也实现 Add。

这是普通 Rust 的二元表达式和闭包。没有将操作符语法宏改写成 children，不使用 view! 或 ;;。不加 component 宏也能编译，LSP 也能分析；component 仍用于隐式身份。

## 已验证

运行：

```sh
cargo run -p plico-core --example operator_children
cargo test -p plico-core --test operator_children
python3 scripts/experiments/check-operator-children-lsp.py
python3 scripts/experiments/check-operator-children-compilation.py
cargo test -p plico-cli native_operator_children_are_flat_and_idempotent
cargo run -p plico-cli -- fmt --check crates/plico-core/examples/operator_children.rs
```

- rust-analyzer 1.99.0 (b940084 2026-09-28)，真实 stdio LSP，不用源码字符串搜索模拟补全。普通函数、component 函数分别测试，共 26 项全部通过。
- 外层 chain 方法、闭包内控件函数、嵌套控件、builder 方法、context 方法、局部捕获、可变捕获、重命名 context 的补全正常。
- 嵌套 context 的 hover 能确定 UiContext，text 能跳转定义；刚输入 `text(` 的不完整语法仍提供 signature help。
- 运行时验证嵌套/分支、key 列表、稳定帧不创建节点，以及条件变化前后节点身份和队列点击仍正确。
- FnOnce 可以消费拥有的 String，返回值原样保留；自定义 panel 工厂返回 Layout 时能保留借用和子节点身份。
- 编译负例验证：零参数闭包 E0593；错误捕获外层已借用 context E0500/E0499；消费后的结果继续 padding 为 E0599；font_size 类型错误为 E0308。负例检查使用实际 Plico 依赖，不是另造简化类型。

LSP 探针测试 completion/hover/definition/signatureHelp；错误拒绝由单独 cargo check 探针验证。没有把编译器诊断冒充 rust-analyzer 原生实时类型诊断；每个编辑器的 diagnostics/check 配置仍决定红线何时出现。

测试中发现一个需要修复的边角：原 component 宏只识别 children 方法中的 context 重命名，不能识别 Add 右侧的 |inner|。已让身份生成器识别该绑定，并添加条件变化后身份稳定的回归测试。此处识别绑定用于身份，不是改写 Rust 语法。

## Formatter

仅在 `#[component] #[rustfmt::skip]` 选中区域生效。rustfmt 默认往往把 + 移到下一行；plico fmt 现在对右侧为 block closure 的原生加法表达式增加 whitespace-only 排版：

- 头部能放下时合并为 `column(cx).gap(8) + |cx| {`，正文只增加一层缩进。
- 长链按既有宽度规则继续平铺折行；尾部能放下就保留 `+ |cx| {`，放不下时保留单独一行。
- 保护 raw string、普通字符串和注释；操作符间有注释时保留该边界。
- 60/80/120 列测试重复格式化结果一致，并沿用 formatter 的 token-preservation 检查。已有 formatter 回归也通过。

示例文件已由实际 CLI 格式化，再执行 --check 通过。没有扩展 save hook 或编辑器插件。

## 采用后的约定

双分号布局已经移除，宏和 formatter 遇到旧语法明确报错，避免被当作无关 block 执行。容器默认使用 `+ |cx| { ... }`；底层 children 保留，返回值仍保持各 builder 的真实类型。component 会将独立 operator 声明的结果明确丢弃（等价 let _ = ...），避免 Rust 的 unused arithmetic operation 警告；赋值和尾表达式保留返回值。宏不负责把 + 语法变成 Rust。

已移除旧空语句 lowering 和 formatter 的标记恢复流程。普通 Rust 控制流、单分号 block 和宏中的字符串保持原含义。空子内容使用 |_cx|，避免 unused_variables。尚不支持 `+ { ... }` 自动闭包，不在本方案内。
