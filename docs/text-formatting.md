# 动态文字与格式化分配

静态和已有字符串用 `text(cx, "Title")` / `text(cx, model.name.as_str())`。动态格式化可以使用：

```rust
use plico_core::prelude::*;

#[component]
fn counter(cx: &mut UiContext, count: u64) {
    text!(cx, "Count: {count}").font_size(20);
    text!(cx, "Progress: {:.1}%", 42.25);
}
```

text! 接受 Context 首参，剩余参数遵循标准 format_args!，包含捕获变量、命名参数、宽度/精度和 Debug。宏在 #[component] 内参与静态身份生成；动态列表仍需显式业务 key。函数 text 和宏 text! 位于不同命名空间，可以同时通过 prelude 导入。

宏展开为 widgets::text_fmt(cx, format_args!(...))。参数同步写入 Engine 的复用 String，返回正常 Text builder，因此参数临时引用不进入 retained tree；builder 提交时把内容借给声明协调器，比较并按需复制到节点已有文本容量中，然后把 scratch 交回 Engine。纯字面量走借用路径。核心没有新增 unsafe 或保留格式化闭包。

这省掉每次 format! 创建临时 String 的分配，不省掉格式化计算、内容比较或变化文本的必要复制。首次创建、缓冲或节点容量增长仍会分配；自定义 Display 实现内部也可能自行分配。只有内容不变时才跳过节点文本写入和测量。热重载 JSON 序列化和 renderer 的开销不在这个优化范围内。

`cargo run -p plico-core --example allocations` 使用独立进程的分配计数器，预热后验证 10,000 条文本：

| 声明方式 | 分配次数 | 请求字节 | 节点文本写入 |
| --- | ---: | ---: | ---: |
| 内容不变的 format! | 10,000 | 140,000 | 0 |
| 已有字符串借用 | 0 | 0 | 0 |
| 内容不变的 text! | 0 | 0 | 0 |
| 全部内容变化、容量足够的 text! | 0 | 0 | 10,000 |

这是 renderer-free 核心声明与简化布局样本的计数，不是整个应用或真实文本 shaping 的零分配保证。格式化更长的新内容时需要允许扩容；此处没有采用截断或固定栈容量来换取表面零分配。
