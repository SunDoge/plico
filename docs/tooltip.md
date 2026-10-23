
# Tooltip

Tooltip 是非交互提示，不需要应用保存 open 状态。内容闭包立即执行，返回其中控件的响应；宿主决定何时显示提示。

```rust
use plico_core::prelude::*;

#[component]
fn toolbar(cx: &mut UiContext) {
    let save = (tooltip(cx, "Save the current document")).children(|cx| {
        button(cx).text("Save").primary().response()
    });
    if save.clicked() {
        // 发送应用命令。
    }
}
```

静态调用由 component 生成身份；动态列表仍使用 `.key(business_id)` 或外层 keyed。描述可以借用字符串，不要求 static，也不保留应用回调。包装组不绘制描述或增加 padding/gap；它仍是一个真实布局节点，可能影响原有父容器的 flex 分配。把需要一起获得提示的内容放在组内，实际样式继续配置内部控件。

## 行为与样式

- 指针进入后等待 600ms；键盘可见焦点立即显示。工具栏在另一条提示消失后的 400ms 热区内切换时直接显示。
- 同时只显示一条，嵌套包装组选择最内层描述。禁用控件、空描述、已卸载或完全被裁剪的锚点不显示。
- 离开、窗口失焦、按下控件、Enter/Space 激活或 Escape 隐藏；Escape/激活后的提示在指针或焦点离开该组之前不会立即重现。
- 输入框组合期间，第一个 Escape 留给 IME；提示显示在 Modal 中时，Escape 先关闭提示，下一次才交给 Modal。
- 提示不抢焦点、不成为 Tab stop、不接管 pointer capture。它不在事件命中列表中：提示覆盖的底层控件仍能收到真实指针操作。
- 默认在锚点上方居中，空间不足时放到下方，再限制在窗口内。宽度最多 320px，长文本换行；反色背景/文字随主题切换。窗口缩放、锚点位置或内容改变会重新测量。
- 提示内容仅为文字，没有表单、按钮或关闭动作。需要交互内容时使用 Popover。

## 宿主、时钟与描述

核心通过 `Kind::TooltipAnchor` 和节点 text 传递描述，沿用 ViewNode 及逻辑 key，不增加字符串字段到每帧复制的 Style。组内内容仍是普通 immediate 声明。

宿主用注入 Clock 的 Duration 管理延迟、热区与抑制状态，不创建异步任务或系统 timer。等待期间当前宿主通过 has_anim 请求 rAF，约 600ms 内仍会采样帧；这不是定时器休眠优化，但这些中间帧不执行应用声明或主树布局。提示出现后没有待处理动画/期限时回到空闲。

绘制使用同一 Taffy/Parley/Vello 管线中的一个宿主持有叶子，输出追加到返回的 placed list，保留主树的命中与滚动列表。它不被模块序列化，也不作为 Modal/Popover 焦点层。文字、主题、锚点和窗口不变时复用提示排版；BackendStats.tooltip_layout_runs 与主树 layout_runs 分开计数。custom shader 场景也包含该层，显示/隐藏会正确使 scene cache 失效。

辅助功能树保留稳定的 Tooltip 描述节点，控件通过 described_by 引用；描述在视觉提示未出现时也可获取，不把描述替换成按钮名称。描述节点没有动作。Modal 投影仍只暴露当前可访问子树。

热重载模块只声明描述与稳定身份；宿主保留焦点/悬停计时。描述源代码变更会更新已显示提示；控件身份变化或应用状态重置时允许丢弃提示状态，符合 best effort 策略。

## 参考、运行与边界

参考本机 MyGO ui/tooltip.go 的 600ms/400ms、键盘焦点、最内层提示、激活抑制与指针穿透；Wabou ui/src/components/tooltip.tsx 的非交互层、描述关系和焦点不恢复规则。没有复制其中的 Go engine 或 Solid signal/timer 实现。

```sh
cargo run -p plico-gallery
# Controls：悬停或 Tab 聚焦 Increment / Reset
cargo run -p plico-gallery --example capture -- --page controls --tooltip --out /tmp/tooltip.png
cargo test -p plico-backend --test tooltip
cargo build -p plico-cli
python3 scripts/check-modal-reload.py
```

八项集成测试覆盖长空闲后的时钟延迟、等待帧不重声明、键盘/描述/Escape、热区/嵌套、真实指针穿透、禁用/卸载、Modal 与 IME、窗口失焦、长文本/缩窗，以及与 WGSL 同屏的实际 GPU 像素及 scene cache。真实 cdylib 测试在重载前后比较已显示提示的内容及宿主焦点；截图位于 reports/tooltip。开发宿主 headless JSON 增加 focus 动作，并返回可见 tooltip 的描述、bounds、focus 作为验证证据。

当前不提供自定义 placement/delay、富文本、shortcut keycap、入退场 tween 或视觉关闭延迟。只验证了本地 headless GPU 与语义投影，没有把这些结果当成所有桌面平台或读屏软件的认证。
