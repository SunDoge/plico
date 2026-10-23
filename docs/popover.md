# 锚定 Popover


`popover` 同时声明按钮和浮层，内容闭包在当前帧立即执行。状态放在应用结构中，可随 best effort 热重载恢复；不保存应用闭包或 arena 节点 ID。

```rust
use plico_core::prelude::*;

#[derive(Default)]
struct Settings {
    popup: PopoverState,
    notifications: bool,
}
impl Settings {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        (popover(cx, &mut self.popup, "Quick settings").width(240.)).children(|cx| {
            text(cx, "Workspace").weight(650);
            switch(cx, "Notifications", &mut self.notifications);
            if button(cx).text("Done").primary().clicked() {
                cx.close_popover();
            }
        });
    }
}
```

静态调用由 component 生成身份，动态列表使用 `.key(business_id)`。
PopoverState 复用 ModalState 的 open/close/is_open 和退出期限。构建器提供 width、disabled、motion、key；响应的 inner 为未挂载时 None，挂载时内容返回值，dismissed 表示消费了外部关闭请求。

## 定位与输入

- 根 portal 不占触发按钮父容器的布局，不受其裁剪。默认左对齐按钮下方，间隔 6px，窗口边距 8px；下方不足且上方更宽裕时翻转，横向避让窗口边缘。
- 定位修改真实 Taffy absolute inset 和高度约束，再生成 placed list。绘制、命中、滚动、语义 bounds 使用同一份几何数据。高度超限时内部滚动；缩放窗口后重新测量，不沿用旧窗口的高度上限。
- 按钮按 Enter/Space 可打开，打开后聚焦首个启用输入框或控件。Escape、完整的外部主键点击、内容中的 close_popover 关闭；外部点击不会穿透触发后面的操作。
- Tab/Shift+Tab 关闭并从触发按钮继续普通焦点顺序。退出动画结束不再抢回焦点。普通关闭会恢复触发按钮，嵌入 Modal 时仍保留父层输入限制。
- 浮层是非 modal Dialog，没有遮罩。触发按钮暴露 expanded/controls。位于 Modal 内的 portal 在辅助功能树中归属包含触发按钮的 Dialog，避免被 modal 投影隐藏。
- 默认 Quick 淡入淡出；宿主采样注入时钟，中间动画帧不重新声明应用或运行 layout，期限到达时再卸载。motion(Motion::None) 立即关闭。

## 热重载与成本

按钮和 portal 用相同逻辑锚点 token 关联，token 来自调用身份、作用域和逻辑祖先，不含本地实体句柄。Style 中的可选 anchor_id/popup_anchor 随 ViewNode 传输，旧数据通过 serde 默认值兼容。状态无法反序列化仍按现有策略重置。

当前定位在几何或滚动变化时增加一次测量，以及每个打开浮层一次放置布局；Taffy 可以复用未变节点，但尚未针对大量嵌套浮层做性能优化。纯 paint/opacity 动画不走这条定位路径。

这是通用交互内容浮层的第一版。Select/Menu 已在其上实现专用键盘行为和角色，参见 [select-menu.md](select-menu.md)；[Tooltip](tooltip.md) 使用独立的宿主计时与 pointer-through 提示层。当前不提供任意自定义触发按钮、右对齐/侧边 placement、锚点滚出裁剪区自动关闭或独立层排序 API。不要用 Popover 冒充这些专用控件。

## 验证

```sh
cargo run -p plico-gallery
# Controls → Quick settings
cargo run -p plico-gallery --example capture -- --page controls --popover --out /tmp/popover.png
cargo test -p plico-backend --test popover
cargo build -p plico-cli
python3 scripts/check-modal-reload.py
```

六项集成测试覆盖翻转避让、定位后的真实指针命中、窗口缩放、禁用、Tab/焦点恢复、Modal 内语义投影与 Escape、虚拟时钟动画无重声明/布局。真实 CLI/cdylib 测试覆盖打开状态、锚点 token 和关闭动作的重放。light/dark-small GPU 截图位于 reports/popover。
