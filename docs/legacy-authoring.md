# Immediate API、宏与格式化

当前主线使用 `plico-core`；普通项目入口见 [编写方式](authoring.md)。

## 原生 operator children

容器配置完成后，使用 `+ |cx| { ... }` 声明子内容。这是普通 Rust 的 Add 和 FnOnce，不需要宏把自定义语法转换为闭包。

```rust
use plico_core::{icons, prelude::*};

#[component]
#[rustfmt::skip]
fn settings(cx: &mut UiContext) {
    column(cx).gap(12).padding(16) + |cx| {
        text(cx, "Settings").font_size(24);
        button(cx).primary() + |cx| {
            icon(cx, &icons::SAVE);
            text(cx, "Save");
        };
    };
}
```

`#[component]` 负责隐式身份和 context 作用域。闭包参数可重命名为 inner，动态列表继续提供业务 key。独立声明语句的返回值由 component 丢弃，避免 Rust 对非 unit 加法结果的 unused_must_use 警告；赋值、尾表达式仍保留原返回类型。不使用宏也可编译，需要时用 `let _ = ...` 明确丢弃非 unit 结果。

Layout 返回闭包结果；Button/ButtonBase 返回 InnerResponse，Modal/Popover 返回 ModalResponse，Tooltip 返回闭包结果。需要读取时给整个表达式加括号或绑定变量：

```rust
let result = button(cx).primary() + |cx| {
    text(cx, "Save");
};
if result.response.clicked() {
    // ...
}
```

`+` 消费 builder，children 是配置的结束点。自定义容器可以返回 Layout，或者为自己的 builder 实现 Add<F>，直接委托已有 children。原生 `.children(...)` 继续作为底层 API。旧双分号布局已经移除，component 和 CLI formatter 都会给出迁移错误，避免当成两个无关语句运行。

`#[rustfmt::skip]` 仅用于保留组件排版。编译和语义分析不依赖 formatter；去掉 skip 后，普通 rustfmt 仍能正确处理代码，只是会按自己的规则折行。

```sh
plico fmt path/to/component.rs
plico fmt --check
```

CLI 只对 component + skip 函数执行平铺布局，其余代码使用 rustfmt。不需要 Python、uv 或 format-on-save。真实 LSP 与编译负例见 [验证记录](experiments/operator-children.md)。

## Button 是内容容器

`text(cx, content)` 保留必需内容参数。`button(cx)` 独立构造，提供标准内容简写：

```rust
button(cx).text("Save").primary();
button(cx).icon(&icons::SAVE).text("Save").primary();
button(cx).icon(&icons::SETTINGS).aria_label("Settings").ghost();
```

简写也生成真正的 SVG/Text 子节点，和自定义 children 共用按钮容器。
自定义 children 替换简写内容；文字内容可推导无障碍名称，`.aria_label(...)` 显式覆盖。
纯图标按钮应显式命名。旧 `ui.button("...")` 保留兼容。

## 统一入口

`view!` 已移除。布局由普通 builder、`+ |cx| { ... }` 与底层 `.children(...)` 组成。
Context 首参独立控件、真实 builder 类型、同步执行与身份规则保持不变，详见
[组件](components.md)、[身份](identity.md) 和 [文字格式化](text-formatting.md)。

## rustfmt 的边界

Plico 核心当前使用默认 rustfmt，不再依赖为平铺 chain 调整的 use_small_heuristics="Max"。内置后端仅保留 edition/style_edition 2024 设置。Max 曾用于原实验，倾向用满宽度但不能保证任意长链都平铺。Rustfmt 仍会按表达式、闭包和宽度规则折行；不存在全局通用的“106 列才竖排”阈值。

原 view-repr 的特定样本在 98、99、106 列出现不同排版，是固定配置和表达式的观测结果，不能泛化成 Rustfmt 规范。

## 专用 formatter

CLI 使用 Tree-sitter 定位同时带 `#[component]` 和 `#[rustfmt::skip]` 的函数。
它借助 rustfmt 处理普通 Rust，再按宽度装行，续行继续平铺多个调用，并对齐 operator children 正文。
输出只替换受控函数，保留其他区域字节，并检查语法与格式化幂等。
字符串、注释与宏输入受到保护。编译不依赖专用 formatter，普通 children 可以直接用 rustfmt。

## LSP 证据与后续要求

原 view-repr 通过真实 rust-analyzer JSON-RPC 测试，覆盖 scope body、嵌套、长链、未完成宏、状态与动画/任务 API 补全，以及 formatting edits。那是原版本证据，不代表重命名和迁移后的完整 LSP 回归已经执行。

将来迁入 formatter 时应恢复这些测试，包含 UTF-16 编辑位置处理；新 API 是否有补全要实测，而非仅凭宏能编译推断。

当前 component/text 过程宏已由真实 rust-analyzer 验证 18 个补全样本，包括旧 Ui 方法、独立 Context 首参函数、builder、布局头、now、request_animation_frame 、text! 后的样式链和 toggle 配置/响应、icon 构造、SVG 颜色和完整 Lucide 常量。运行 scripts/check-lsp.py，报告在 reports/macros/lsp.json。它不包含原实验全部 formatter、导航或任意残缺宏测试。
