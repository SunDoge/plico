# trailing_closure 展开器评审

用户提供 expand.rs（2026-10-21）。未将第三方源码加入仓库，也未改变已选的 operator children 路线。在 /tmp 独立 Cargo package 中运行其 10 项原测试，并新增 2 项边界测试，12 项通过；没有增加项目依赖。

## 机制与 LSP

通过 syn::Fold 自底向上遍历 Item。只把逻辑或 BinOp::Or 中“左侧 Call/MethodCall，右侧 Block/Closure”的形状转换为追加调用参数：

```rust
call0() || { 40 + 2 }
// -> call0(|| { 40 + 2 })
map() || |value| { value + 1 }
// -> map(|value| { value + 1 })
```

输入是 Rust 能解析的表达式，属性宏先于类型检查展开，所以原始表达式不必满足 bool 逻辑或类型要求。这里没有重载 ||。

原 Call、Closure 和 Block 的 AST 被移动复用；parse_quote 生成零参数闭包时插入 #block，也保留 block 内原 token 的 span。只新增闭包外壳、参数逗号等 token，因此 rust-analyzer 有条件将原位置映射到展开后的表达式。这份文件没有专用 LSP 接口或特殊补全实现。

截图只展示整数字面量的成员补全，不能据此声称隐式 Context、控件函数、不完整调用及过程宏解析失败时都正常。expand_item 接收已经解析好的 Item，未提供宏入口，所以不能从该文件判断残缺输入恢复策略。

## 对 GUI 的两个限制

1. **追加给最后一个调用，不是追加 children。** 已新增测试确认 `column(cx).padding(20) || { body }` 会展开为 `column(cx).padding(20, || { body })`。要适配现有 Plico API，需要写 `.children() || |cx| { body }`，或者另做针对 builder 的转换；该文件本身没有这些规则。零参数闭包也不能解决 builder 已借用 &mut Context 后再次捕获同一个外层 Context 的借用冲突。
2. **无法区分所有真实逻辑或。** 测试确认 `ready() || { true }` 被改成 `ready(|| { true })`，通常会破坏原本合法调用的签名；而 `flag || { true }` 不变。源码用 `(ready()) || { true }` 手动退出该规则。它没有类型信息，不能自动知道调用者的本意。

另外，连续 `both() || { 1 } || { 2 }` 会为同一个调用追加两个闭包，这是该通用语法有意支持的行为；不是 GUI 的“children 消费 builder 后结束”约束。return/?/break 等控制流仍按展开后的闭包语义理解。

## 判断

实现简洁，AST 复用与 span 保留的方法值得参考，说明可解析的 Rust 外形加属性宏也能获得一定 LSP 支持。但它解决的是通用函数末尾参数的书写，不直接解决 Plico 的借用和容器配置问题。`+ |cx|` 保留正常 Add/FnOnce 类型语义、显式子 context，并避开逻辑或的视觉歧义。后续如做对照，应使用同一组真实 Plico LSP/借用/格式化探针，而不能只比较数字补全截图。
