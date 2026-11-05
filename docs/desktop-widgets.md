# Desktop widgets

Gallery 的 Desktop patterns 页面提供 tabs、toolbar、toast 和 drag surface 示例。

## Toast presence

`ToastHost` 保存通知；在视图中调用一次 `toast_viewport(cx, &mut host)`。
Viewport 挂到窗口根节点，因此不会受调用处的小尺寸或滚动裁剪限制。

- 入场和退场使用透明度与 16 px 的垂直位移，时间来自注入的 Clock。
- 时长使用主题的 `interaction_ms`；设为 0 可关闭过渡。
- 关闭后立即停止接受输入，但保留卡片直到退场结束。同 ID 的通知在退场期间重新加入，会从当前进度反向进入，保留节点身份。
- `len()` / `is_empty()` 只统计有效通知；`toast_viewport` 在仍有退场卡片时也返回 true。
- 指针停在卡片正文或按钮上时暂停超时；离开后继续剩余时间。排队而尚未显示的通知不消耗寿命。
- 静止的 sticky 通知以及悬停中的静止通知不持续请求帧；退场结束后也停止请求。

容器通过 `UiContext::hovered_within()` 观察整个卡片的命中范围。
Runtime 只查询主动观察的容器，由 Adapter 反馈可见几何，覆盖非交互正文的悬停。

可重现的离屏入场预览：

```sh
cargo run -p gallery -- --scenario apps/gallery/scenarios/toast-motion.json --capture reports/toast-motion.png
```

核心测试覆盖动画中断、同 ID 复用、闲置后关闭、超时和零时长；Runtime 测试使用虚拟时钟验证真实几何悬停、帧唤醒和 portal 裁剪。
