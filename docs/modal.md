# Modal 与窗口浮层

`modal(cx, &mut ModalState)` 是 Context 首参的独立组件。内容闭包在当次声明立即执行，不保存为回调；普通控件可以直接组合进去。

```rust
// 放在应用状态里：dialog: ModalState
if button(cx).text("Edit preferences").clicked() {
    self.dialog.open();
}

(modal(cx, &mut self.dialog).title("Preferences")).children(|cx| {
    checkbox(cx, "Notifications", &mut self.notifications);
    if button(cx).text("Done").primary().clicked() {
        cx.close_modal();
    }
});
```

ModalState 提供 open / close / is_open，记录可见性和退出期限，可序列化用于 best effort reload。它不是业务数据容器，内容状态仍由应用持有。不要用每帧新建的 ModalState。动态列表中的弹窗用业务 key，静态调用由 component 宏生成身份。

## 已实现

- portal 直接声明到窗口根部，保留逻辑祖先及调用位置导出的身份；不使用 arena handle 作为可恢复身份。不受原父容器裁剪或滚动影响，也不占原布局的空间。
- 根节点将 overlay 排在普通内容之后。嵌套 modal 按声明顺序堆叠，最上层独占输入；每层有独立遮罩和 Dialog 语义。
- 默认居中、440px 最大宽度、20px 窗口边距。内容超高时在弹窗内部滚动；窄窗口文字和动作可以换行。
- 打开时优先聚焦第一个启用的 TextInput，否则第一个控件；Tab / Shift+Tab 在最上层循环。关闭完成后恢复前一层焦点，原目标已卸载时不恢复失效 handle。
- Escape、标题栏 Close dialog、内容中的 close_modal，以及一次完整的遮罩主键点击都能关闭。取消指针序列不触发关闭。可分别设置 dismiss_on_escape(false) 和 dismiss_on_backdrop(false)。IME 组合期间的第一个 Escape 留给编辑器。
- 背景鼠标、滚轮和语义动作被隔离。退出期间仍阻止穿透，弹窗内容不能再次触发动作。背景布局、输入值和滚动位置保留。
- 默认 Quick 淡入/淡出。宿主按注入 clock 采样 opacity，动画中间帧不重复执行应用声明；到退出期限再声明一次，卸载内容并恢复焦点。motion(Motion::None) 可立即开关。

ModalResponse.inner 是内容返回值的 Option（未挂载时为 None）；dismissed 表示此次声明消费了关闭请求。退出期间内容仍会在必要的声明中执行，应用应像其他 immediate 代码一样，把副作用放在事件条件内。

## 热重载

Overlay/Dialog、关闭请求及退出期限都走现有 ViewNode / 逻辑事件协议。宿主保留焦点栈和属性动画，ModalState 随应用状态恢复；期限到达时 ReloadView 重新调用模块，移除退出节点。不能反序列化的应用仍按原有策略重置。

这不改变输入绑定的限制：TextBuffer 目前只支持静态原生 View，不能通过热重载 ViewNode 传输。Gallery 的可重载弹窗使用 checkbox；独立原生示例演示真正的文本输入。

## 运行与验证

```sh
cargo run -p gallery
# Controls → Edit preferences → Save preferences → Apply changes
cargo run -p plico-vello --example modal
# 原生输入、Enter 保存、取消和焦点恢复
cargo run -p gallery --example capture -- --page controls --modal --out /tmp/modal.png
cargo build -p plico-cli
python3 scripts/check-modal-reload.py
```

集成测试覆盖脱离裁剪、嵌套、焦点循环与恢复、背景语义动作隔离、遮罩取消序列、IME、关闭策略、小窗口滚动、虚拟时钟退出卸载及中间帧不重新声明。协议测试覆盖应用重建后的身份和退出期限。真实 CLI/cdylib 重载验证记录在 reports/modal/reload.json；light/dark/small 截图在同目录。

锚定 Popover 已实现，参见 [popover.md](popover.md)。专用 [Select/Menu](select-menu.md) 也已接入。显式 initial/return-focus 目标、标题栏自定义、独立浮层排序 API、transform 入场动画尚未提供。窗口缩到无法容纳边距和标题的极端尺寸不作可用性保证。已有宿主 reduced-motion 会停止属性补间；需要同时取消退出等待时，应用使用 motion(Motion::None)。
