# 状态、事件与提交


## 当前状态模型

当前 gallery 使用 Gallery struct 的普通字段。Ui 的引用和布局闭包只活在当前声明调用中。Button::clicked() 同步消费 Engine 中已排队的点击；switch / checkbox 接受短期 &mut bool，修改后返回是否发生变化，不保存借用。

组件局部状态可用 `ui.state`；业务持久状态与宿主资源的边界见 [local state 与任务](local-state-and-tasks.md)。

## 帧与事件顺序

核心 frame 执行一次 view，随后提交子节点顺序、清理未声明节点，并选择性执行模拟尺寸计算。frame_declarations 跳过模拟测量，由实际后端处理尺寸。

View::show / App::show 现在返回 ()。消费点击/切换事件自动设置 Changes::VIEW，Source/应用协议最多重执行 8 次直到没有 VIEW 请求；稳定帧只声明一次。文字即使在事件处理前声明，也会在同帧更新。事件只消费一次，业务副作用应放在消费成功的分支。按钮 clicked、受控控件 changed 等事件消费已经自动请求 settle，业务分支不必重复 request_rebuild。非事件变更需要 ui.request_rebuild 或宿主 request_rebuild；没有自动检测任意 Rust 字段写入。

平台可能在一次呈现前交付多个事件。prepare_for_event 必须提交前一事件引起的状态和布局，再处理下一事件，防止继续用过时几何命中。当前 Wabou adapter 已有对应回归测试。

不承诺 view 内任意副作用可安全重复执行。自动稳定化、任务和 hot reload 的迁移必须共同定义最终提交边界，不能通过盲目重跑业务代码保证一致性。

## 缓存失效

| 输入变化 | 至少需要处理 |
|---|---|
| 文本、字号、可用宽度 | 文字测量，必要的布局和 paint |
| gap、padding、显式尺寸、子节点顺序 | 相关布局和 paint |
| 前景/背景颜色 | paint，不触发文字几何重测 |
| 主题 | 依赖主题的 paint；若未来包含字体/尺寸 token，还要相应测量 |
| focus / hover / pressed | 当前控件的交互外观 |

核心的模拟缓存验证的是失效关系，不是真实 shaping。Wabou adapter 的文字测量上下文发生几何变化时显式 mark_dirty；纯颜色变化只替换 paint 数据。

“没有重测”不等于“没有遍历”。“没有 paint 数据写入”也不等于“没有场景构建或 GPU 工作”。未来统计应分开声明、协调、测量、布局、场景生成、上传与呈现。

## 卸载与恢复

当前未声明节点会在核心帧提交时删除，后端也移除对应记录。节点重新挂载获得新的 generational handle。框架状态、任务和动画分别如何处理隐藏与卸载，尚待统一；不能仅凭同一个字符串 key 就假定全部数据永久保存。

未来状态可按逻辑 key、类型标记和版本做 best effort 恢复。业务持久状态、控件临时状态、焦点/选择、动画轨迹与异步任务应分别定义生命周期，而不是打包成一个任意序列化的对象。

## 失败边界

核心会报告重复 key、非法尺寸等错误，但声明阶段不是完整事务：中途可能修改暂存记录，view 的业务副作用不会回滚。恢复后缓存正确性已有测试；不代表可回滚任意应用操作。

后续应明确失败时能否保留上一份可呈现场景，以及新帧资源/任务在何时正式提交。错误恢复策略必须与稳定化协议一起设计。

## Bitflags 与时间

核心 Changes 包含 VIEW / LAYOUT / PAINT / SEMANTICS / ANIMATE / STRUCTURE。VIEW 是收敛请求，ANIMATE 是下一帧请求；其余由声明差异与缓存统计产生。应用无需手工维护 changed。后端合并多次稳定化声明的 dirty journal，局部比较 retained 节点；结构变化或声明失败后的恢复采用完整同步。详见 [增量执行](incremental.md)。

时间来自宿主注入的 Clock 和 FrameTime，同帧收敛不推进时间，参见 [clock.md](clock.md)。自动身份见 [identity.md](identity.md)。

## Hover 与点击

```rust,ignore
let response = button(cx).text("Open").response();
if response.hovered() {
    // 声明预览或提示，不要在每次声明时重复发起任务。
}
if response.clicked() {
    self.open();
}
```

`Response` 是 Copy 值，不持有 Ui 借用或闭包；读取 hovered 不消费事件。
悬停是当前宿主输入状态，点击也可以来自键盘或无障碍操作，因此不要嵌套判断。
禁用按钮不报告 hovered；指针离开/取消后会清除。普通 `button` 与自定义
`button_base(...).children(...)` 的 response 都支持 hovered。容器 response 不表示
任意矩形命中。纯外观用 `.hover_background(...)`，不必在业务代码分支。

静态宿主将 hover identity 同步到 Engine；模块协议按逻辑路径传递，native reload
与 split wasm replay 在路径变化时自动稳定化，不依赖模块侧 arena handle。
Gallery 的 Controls → Hover and activation 提供示例，不需要 request_rebuild。
