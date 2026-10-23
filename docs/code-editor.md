# Native code editor

状态：第一版 native surface 接入，可运行与测试。不是完整 IDE，也还不是核心
`widgets::code_editor(cx, ...)` 节点；不宣称支持 Web 或跨模块编辑器序列化。

## 运行与接入

```sh
cargo run -p plico-gallery --example code_editor
```

新增 `plico-editor` crate，公开 `CodeEditorState` 和实现后端 `Widget` 的
`CodeEditor`。文档类型不暴露 Floem 类型；clone 共享文档、选区、revision 和撤销历史。

```rust,ignore
let document = CodeEditorState::new("fn main() {}\n");
let editor = CodeEditor::new(document.clone()).language("rs");
let source = SurfaceSource::new(view, editor, "code-editor");
```

`view` 声明一个显式尺寸的 `column(cx).key("code-editor")` 作为布局 slot。
完整示例在 `apps/gallery/examples/code_editor.rs`，使用 context 首参和 children。
这个 key 是当前 native surface 桥接所需的显式绑定，不是要求普通控件手写身份。
目前每个 SurfaceSource 只承载一个 native widget，沿用其普通、未变换 slot 的限制。

宿主保留 document handle，可以通过 `text()` 显式导出、`revision()` 检测编辑、
`selection()` 查询 anchor/head、`selected_text()` 导出选区，以及 undo/redo。
选区为 UTF-8 byte offsets；`select` 拒绝非字符边界与越界位置。
外部修改后调用方需要请求一帧；handle 本身不创建后台线程或事件循环。

## 实现边界

- 编辑存储、Delta、修改范围和撤销历史来自 `floem-editor-core`，固定上游
  `lapce/floem` commit `1351ffb162faeb8be983f1301d718a0d5fb59c27`。
  使用其 `lapce-xi-rope`，没有引入整套 Floem GUI、reactive 或窗口。
- Plico 负责单光标输入、焦点、剪贴板、滚动和 native IME 适配；不把 Floem
  widget 的完整功能当作已移植。第三方依赖许可证见上游该 commit 的 manifests/LICENSE。
- Parley 对可见逻辑行排版，同一份 layout 用于指针命中、光标、选区和绘制。
  不用字符列乘固定宽度来计算鼠标命中。行号固定，正文横向滚动，暂不软换行。
- 文字布局缓存可见行，编辑后或滚动到新行时刷新；光标移动不必重新解析高亮。
- 基础高亮使用 syntect；修改后按行重新解析整个文档并缓存 spans，并非 Tree-sitter
  增量高亮。超过 256 KiB 自动只显示纯文本；虚拟行不等于高亮已增量化。
- 支持左右字素移动/删除、上下与翻页的视觉列保持、Home/End、Shift 扩选、
  鼠标拖选、全选、剪切复制粘贴、Enter、Tab 插入四个空格、撤销重做。
- IME preedit 使用临时 Rope 投影替换选区，既不修改已提交文档，也不进入撤销历史。
  Commit 一次替换并独立成组，取消/失焦丢弃投影。支持候选框 caret、周围文本与
  UTF-16 选区反馈，以及 DeleteSurrounding；暂不支持带换行的 preedit。
- 本轮没有实现自动缩进/补全、搜索、双击选词、拖选边缘自动滚动、多光标、折叠、
  LSP、文件 IO 或完整无障碍编辑语义。撤销历史尚未增加内存预算。

## 热重载与生命周期

编辑文档是宿主对象，不做每帧 JSON 往返，也不让 UI 模块持有其内部 Rust 类型。
同一个 handle 可以随布局/显示隐藏继续使用。但当前 `plico dev` 的 ViewNode
回放协议尚无 CodeEditor 节点和文档 ID 注册表，不能直接在可替换 UI 的 show
函数里创建这一 widget。该宿主桥接与普通 TextInput 是两条不同接入路径。

未做 OS IME 跨平台实测：组合输入的自动化验证使用 native ImeEvent，真实窗口
验证覆盖 Linux X11 键盘/clipboard；不能把前者当作 macOS/Windows 输入法验收。

## 验证

```sh
cargo test -p plico-editor
cargo clippy -p plico-editor --all-targets -- -D warnings
cargo build -p plico-gallery --example code_editor
python3 crates/plico-editor/scripts/check-native.py
```

行为测试覆盖 UTF-8 边界、组合字素删除、选区撤销恢复、redo 分支、IME 提交/取消、
跨行选区组合预览、无 paint 间隔的编辑后命中、翻页视觉列及千行文档滚动绘制。
窗口脚本在隔离 Xvfb 中检查焦点、中文复制粘贴、多行键盘输入和撤销重做，产物写入
`/tmp`（截图位置可由 `PLICO_EDITOR_SCREENSHOT` 指定），不参与提交。
