# 新调用的 LSP 恢复实验

> 历史实验记录：双分号语法及旧 Python formatter 已移除，当前写法与验证见 [operator children](operator-children.md)。

此前测试修改已有调用的函数名、方法名前缀，不能证明从空白位置开始写调用时也能补全。
用户报告在裸尾随块 `view!` 中输入 `text(` 没有补全，追加实验复现了问题。

`check-fresh-call-lsp.py` 通过真实 rust-analyzer 的 didChange、completion 和
signatureHelp，在已有语句前插入八种编辑状态：`te`、`te;`、未闭合的 `text(`、
`text()`、`text();`、`text(c)`、`text(c);` 和 `text(cx, );`。
光标放在相应函数前缀或参数位置。根层与嵌套布局分别验证，参数提示必须来自
新写的 text 函数，不能误算后续 button 的提示。

裸尾随块原实现遇到缺分号或未闭合括号时，整个宏展开失败。尤其未闭合括号会把
后面的语句吞入参数 token group。恢复这些情况需要补语句边界、拆开被吞掉的
参数与后续语句，同时保留原编译错误。已测样本可以修复，但不能据此推断任意
输入都等同于原生 Rust 的工具体验。

`;; {}` 实验原本通过 14/16 状态，只有真正未闭合的 `text(` 丢失上下文和参数提示。
属性宏解析失败时，改为返回原函数并附加 compile_error，让 rust-analyzer 使用
普通 Rust 的恢复能力，随后通过 16/16 状态。负向编译用例确认错误源码仍被拒绝。
这种回退不能保证其他不完整输入都可恢复，也不代表完整的 IDE 功能验证。

最终选择将 `;; {}` 作为正式 component 的可选语法糖，原生 `.children(...)`
保留。实验属性与 view 宏已移除；专用 formatter 仍可处理平铺链。
应用、模板与测试中的 view 调用已迁移到 children / component。
原生 children 在部分不完整调用状态会返回外层 children 的参数提示，作为基线记录；
双分号路线的参数位置要求确实返回新 text 调用的签名。

复现：

```sh
python3 scripts/experiments/check-fresh-call-lsp.py
python3 scripts/experiments/check-component-compilation.py
cargo test -p plico --test component_children --test semi_children --test native_children
```

这些测试直接请求语言服务器，没有验证具体编辑器的自动弹窗策略。
生成报告留在忽略的 reports/ 中，不参与提交。
