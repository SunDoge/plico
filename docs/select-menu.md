# Select 与 Menu


这两个组件使用 Popover 的 portal、定位、事件隔离和退出动画。应用仍在当前调用中同步读取输入并更新普通 Rust 状态，不注册常驻回调。

```rust
use plico_core::prelude::*;

#[derive(Default)]
struct Settings {
    selection: PopoverState,
    commands: PopoverState,
    channel: usize,
    copies: usize,
}
impl Settings {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        let options = [
            SelectOption::new("stable", 0, "Stable"),
            SelectOption::new("beta", 1, "Beta").disabled(true),
            SelectOption::new("nightly", 2, "Nightly"),
        ];
        select(cx, &mut self.selection, "Release channel", &mut self.channel, &options)
            .width(200.);
        (menu(cx, &mut self.commands, "Actions")).children(|cx| {
            if menu_item(cx, "Duplicate").icon(&plico_core::icons::COPY).clicked() {
                self.copies += 1;
            }
            menu_item(cx, "Unavailable").disabled(true);
        });
    }
}
```

## Select

- 选中值由应用持有，类型为 `T: Clone + PartialEq`；仅选择时 clone 新值。选项和标签都可借用，不要求 static。builder 在 `.response()` 或语句末 Drop 时立即声明。
- 每项显式提供业务 key，值应唯一。动态重排保留节点和焦点；显示标签不是身份。静态 Select 调用本身由 component 生成 key。
- 未知值保留，不自动改成第一个选项；显示传入的控件名作为占位。空列表禁用触发器。禁用选项显示并暴露 disabled，但不能通过指针、键盘或语义动作选中。
- 触发器显示当前值和 chevron；辅助功能名称使用传入的控件名，value 为显示值。原生角色为 ComboBox → ListBox → Option，每项有 selected 状态。选择项显示勾号。
- Enter/Space/上下方向键可打开。优先聚焦当前启用的选中项，否则首个启用项；长列表会滚动到焦点。上下键循环且跳过禁用项，Home/End 到首尾，单字符首字母匹配循环定位。
- 方向键或悬停只改变焦点；Enter/Space、指针点击或语义 Click 才提交。Escape 和外部点击取消，Tab 关闭后从触发器继续焦点顺序。
- `.response().changed` 只报告用户提交了不同的值；不是应用级 changed 标记。直接修改应用值不会伪造一次用户选择。
- 默认宽 240px、高度上限 280px；支持 width/max_height/disabled/motion/key。窗口剩余空间更小时进一步约束并内部滚动。

## Menu

`menu` 返回可用 children 构建的 Popover builder，默认高度上限 320px。内容可用 separator 分组，menu_item 支持 icon、disabled、key、clicked/response，激活时自动请求关闭最近的 Popover。

菜单采用 Menu/MenuItem 语义。打开后聚焦首个启用动作；ArrowUp 打开时到最后一项。上下键、Home/End、首字母定位、Escape、Tab 与 Select 共用主机规则。指针悬停能移动动作焦点；随后按 Enter 执行该动作。禁用动作不参与导航。

这不是完整的分级 menubar：尚无标准的左右键子菜单、快捷键标签、复选/单选菜单项，也没有多选、可编辑搜索框、异步选项加载或虚拟列表。单字符定位不等于多字符搜索；所有选项仍被声明和布局。包含普通输入表单时优先用通用 Popover。

## 宿主与热重载

Style.role 是后端无关的可选 WidgetRole，随 ViewNode 序列化；旧快照默认 None。选择和命令使用已有逻辑路径 Click 协议，没有把借用值、闭包、焦点句柄或 GPU 资源塞进模块边界。

宿主持有键盘焦点和滚动位置；应用序列化选中值及 PopoverState。状态无法反序列化时仍按现有 best effort 策略重置。这里没有引入新的异步 runtime。

高度限制同时加入通用 Style.max_height、column/row 的 max_height，以及真实 Taffy max_size.height；Popover 定位重新测量时也保留这个上限。纯 opacity 动画不为此重新布局。

## 参考与验证

参考本机 MyGO `ui/base.go` 的 coreSelectBase（应用持值、trigger/popup、键盘 highlight 与 choose 分离），及 Wabou Select/Popover 的业务选项、禁用项、共享浮层和约束滚动。本实现使用显式可序列化浮层状态及业务 key，不依赖 Go runtime 或保留应用输入闭包。

```sh
cargo run -p gallery
# Controls → Release channel；Quick settings → Actions
cargo test -p plico-vello --test select_menu
cargo run -p gallery --example capture -- --page controls --select --out /tmp/select.png
cargo run -p gallery --example capture -- --page controls --menu --dark --width 640 --height 740 --out /tmp/menu.png
cargo build -p plico-cli
python3 scripts/check-modal-reload.py
python3 scripts/check-lsp.py
```

七项集成测试覆盖禁用跳过、选择/取消、单次 changed、业务 key 重排、首字母导航、菜单首尾、鼠标到键盘交接、空/未知值，以及 80 项列表的选中项可见性、最大高度、缩窗和 Home 滚动。真实模块重载验证打开的 Select/Menu、选中值、原生语义角色和动作回传；截图位于 reports/select-menu。
