# 原生单行输入



`text_input(cx, label, &buffer)` 是 Context 首参的独立控件，可以放在 children 中，支持普通 builder 样式和 `.placeholder()`、`.disabled()`、`.response()`。

```rust
let email = TextBuffer::new();
let password = TextBuffer::secret();
// 缓冲放在应用状态中，不要每次 show 重新创建。
text_input(cx, "Email", &email).placeholder("you@example.com");
if text_input(cx, "Password", &password).response().submitted {
    let password = password.take(); // Zeroizing<String>，移交后台任务
}
```

`InputResponse` 的 changed / submitted 各消费一次；值变更与提交请求同帧稳定化，光标移动不算值变化。缓冲编辑版本进入 paint key，不进入布局 key。占位符改变、绑定切换和空编辑器都能更新 paint。

缓冲使用共享的原生存储，不实现 Debug / Serialize。普通文本可以 text() 读取；secret 拒绝明文 String 读取、复制和剪切，presentation() 只返回按 grapheme 计数的圆点。take() 移出 Zeroizing<String> 并清空编辑状态；替换会释放并清零旧缓冲。接收方仍需管理 SDK 等第三方代码创建的明文副本，不能把这些措施当成任意内存副本均已清零的保证。

Engine 的输入绑定独立于 ViewNode：显示树只包含 accessible label，不包含输入值、缓冲和占位符。原生后端单独 shape 显示文本；密码不会进入文本 layout、display-list text 或 IME surrounding text。secret 向系统 IME 声明 Password purpose。语义节点提供输入角色和 label，尚未提供完整辅助技术编辑接口。

当前支持 Unicode grapheme 删除/左右移动、Home/End、Shift 选择、点击定位、全选、普通文本复制/剪切与粘贴、单行过滤、Enter 提交、基本 IME preedit/commit 和水平跟随光标。组合期间 Enter 留给 IME。光标静态显示。拖动选择、单词导航、撤销、闪烁、完整 marked-range/组合光标以及复杂双向文本编辑仍待补齐。

这条路径目前只接入静态原生 `View`。开发热更新应用的 ViewNode 协议不会传输原生编辑绑定；不能据此宣称动态模块已有可用输入。含密码/会话的应用应使用不要求序列化的 `run_view_with_options`，不要为了热更新序列化凭据。

Warden Desktop 是实际调用方：原生输入 → Rust 队列 → Tokio SDK worker → reply + wake → UI 投影。请求带 generation，取消/锁定后丢弃旧回复；UI 线程不等待网络。这是应用侧 actor，尚不是框架通用异步任务 API。

验证见核心和后端 text_input 集成测试，以及 `reports/macros/lsp.json` 中两个输入 builder 的真实 rust-analyzer 补全用例。测试使用合成值。
