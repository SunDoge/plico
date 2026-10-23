# 流式 Markdown

运行 `cargo run -p plico-gallery`，打开 **Markdown chat**。完整演示集中在 gallery，不增加独立聊天 app。Launcher 的特殊窗口、terminal 的 PTY 和 services-demo 的常驻宿主验证仍有独立入口。

## 组件与文档模型

`plico-markdown` 不依赖窗口、renderer 或异步 runtime。`Stream` 保存追加的 `String`、已显示的 UTF-8 字节位置和逻辑时间；`Document` 保存解析结果。网络服务可以一次追加合并后的 chunk：

```rust
use plico_core::{component, UiContext};
use plico_markdown::Stream;

#[component]
fn message(cx: &mut UiContext, stream: &mut Stream) {
    stream.show(cx);
}

let mut stream = Stream::default();
stream.push("Hello **Rust**\n\n");
stream.push("A second paragraph.");
stream.finish(); // 已收完，剩余展示继续推进
```

组件读取 `cx.now()`，不读取系统时钟。Character 模式使用 Unicode grapheme，Word 使用 Unicode word boundaries，Line 等待换行或流完成，At once 立即显示。组合字符和 emoji 的完整性针对当前已收到的字符串；将来才收到的组合字符无法提前预测。减少动画选项立即显示当前收到的内容并跳过淡入。网络到达和展示节奏独立，finish 不会吞掉剩余动画。

Gallery 通过逻辑时钟模拟每 120ms 到达的一批文字，不连接真实聊天服务。Pause arrival 仅暂停模拟输入，已收到的文字仍会显示。生产应用应通过常驻宿主服务接入异步网络，在更新快照后唤醒 UI。

## 解析缓存

使用 pulldown-cmark 的 offset iterator 解析 Markdown。缓存按最外层块组织：保留前面的块及其字符串分配，重新解析最后一个开放块和新增内容。列表、引用和代码围栏作为完整最外层块处理，不把空行当成无条件安全边界。出现 `[` 时保守地解析全文，避免后置引用链接定义使前面的缓存失效；替换内容也重新解析。空闲帧不解析。`parsed_bytes` 提供累计提交给解析器的字节量，方便验证缓存效果。

节点 key 使用块的源码起点，尾部追加不会替换前面段落的节点。当前是按块缓存的增量方案，长单段落或长代码块仍会反复解析开放块；不是严格的字符级增量解析器。尚未实现 MyGO 的半成品内联语法 healing，因此未闭合强调等语法可能暂时按原文显示。

## 富文本与动画

核心 `TextSpan` 表示段落文本中的 UTF-8 范围、weight、italic 和主题颜色。`text(cx, content).spans(&spans)` 把整段交给 Parley shaping 和换行，不创建逐字符节点。普通文字覆盖同一节点时会清除旧 spans。快照包含可选 spans，native reload 与 WASM split replay 都传递它；验证器拒绝越界、非 UTF-8 边界或错误节点类型上的范围。

当前支持标题、段落、粗体、斜体、代码文本、列表和引用的基础展示。代码块具有独立背景；代码字体、高亮、表格、图片、链接点击尚未实现，不能视为完整 Markdown 阅读器。原始 HTML 仅作为文字展示。列表目前统一使用项目符号，引用与列表内部保留文本换行，未完整表现嵌套结构。

显示动画是按字素/词/行推进正文、每个新块约 240ms 淡入。已经出现的段落追加文字时不会整段重新淡入；尚未支持新增词句各自淡入。段落透明度走现有 presentation 路径；spans 改动目前保守地失效文字测量，尚未细分字体变化与颜色变化。所有展示和淡入结束后停止请求动画帧。模拟输入仍在接收时，gallery 为下一批到达请求帧；真实网络服务应采用事件唤醒。

滚动容器通过 `scroll_to_y` 请求逻辑位置，应用使用现有 viewport 反馈：向上滚动后取消 Follow reply，勾选可恢复。这里采用底部跟随；复杂编辑、图片延迟加载时的阅读锚定仍待实现。

## 文字选择与复制

Markdown 正文默认是一个共享的选择作用域，支持跨段正反向拖选、Shift 点击扩展、Ctrl/Cmd+A 全选当前正文、Ctrl/Cmd+C 复制显示的纯文本，以及 Escape 清除。代码块同样可以选择。复制不包含 Markdown 格式标记，段落之间以空行分隔。输入框保留独立编辑和剪贴板逻辑。

普通应用可在需要选择的布局容器上使用 `.selectable(true)`，其内部普通 Text 节点共享选择；不同容器互不串选。普通标签、按钮内部文字默认不进入这个作用域。选择使用实际 Parley 布局命中位置和计算高亮，沿用现有裁剪与变换。追加文本保留原选区；文本被替换、解析使已选文字重写或节点卸载时清除选区。选择状态留在 host，不进入应用快照。

原生 host 使用系统剪贴板。Web gallery 通过浏览器 copy 事件写入 text/plain，仍需用户触发复制；其他 WebHost 包装应用需要转发 `selected_text()`。当前还未支持双击选词、三击选段、拖动到边缘自动滚动、跨虚拟化卸载段落的选区以及 HTML/Markdown 格式复制。

## 验证

```sh
cargo test -p plico-markdown
cargo test -p plico-backend --test rich_text --test text_selection
cargo test -p plico-gallery --test markdown
cargo run -p plico-gallery --example capture -- --page markdown --markdown-complete --out /tmp/markdown.png
```

测试覆盖逐个 UTF-8 前缀的增量/全文解析一致性、引用定义、开放代码块、列表、源码替换、稳定缓存分配、Unicode 展示、结束后继续推进、checkpoint 后重建解析缓存、富文本进入真实后端和清除、范围校验、完成后空闲，以及向上滚动解除跟随。截图属于本地产物，不参与提交。

原生窗口/系统剪贴板验证：先 `cargo build -p plico-gallery`，再执行 `xvfb-run -a python3 apps/gallery/scripts/check-selection.py`（需要 xdotool、xclip）。脚本在真实窗口中进入 Markdown 页、鼠标选择、全选并复制，读取系统剪贴板验证跨段内容与格式标记剥离。
