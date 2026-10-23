# 兼容路径说明


# Slider 与数值事件

```rust
slider(cx, "Exposure", &mut self.exposure, -1.0..=1.0)
    .step(0.05)
    .width(180.);
```

Context 首参，直接借用应用的 f64。默认连续拖动，方向键以范围的 1/100 调整；step 指定离散步长。支持 vertical(true)、reversed(true)、disabled 和常规样式 builder。reversed 只改变填充方向，不改变数值与指针方向；垂直滑块的较大值在上方。

response().changed 只表示本次消费的输入改变了规范化后的值，不把程序更新或范围规范化当成用户动作。应用无需维护全局 changed。外部值会被限制到范围并按步长取整；端点可准确到达，即使最大值不在步进网格上。范围、步长和值必须有限，步长非负、上界不小于下界；无效声明返回 FrameError。

## 交互与呈现

- 点击轨道、拖动圆形 thumb；拖出控件后继续捕获并限制到端点。主键释放或取消停止拖动，次键不改值。
- Left/Down 减少，Right/Up 增加；PageUp/PageDown 调整十个步长，Home/End 到达端点；Tab/Shift+Tab 使用正常焦点顺序。
- 父容器禁用、modal 输入边界与 focus/IME 切换同样适用。背景滑块不能通过语义动作绕过 modal。
- 主题配色、默认桌面尺寸和 disabled/focus 外观来自同一套控件规则。thumb 位置直接跟随受控值；按压时的轻微增大由宿主 Motion 采样，不重新声明 UI。motion(Motion::None) 可关闭该反馈。
- 语义节点包含 Slider role、label、当前值和上下界。AccessKit 发布 SetValue / Increment / Decrement 并回送数值事件，不把无效 Click 作为滑块动作。

尚未提供双 thumb 范围选择、tick label、自定义 value-text、本地化数字输入或 RTL 专门策略；不将它们视为已经实现。

## 稳定身份与热重载

数值范围属于 Node / ViewNode 的 numeric payload，不放入每个节点都复制的 Style。Style 只保存 paint revision；滑块数值变化不会单独触发布局。应用若同时改变文字或尺寸，它们仍正常测量和布局。

Engine.queue_value 与 queue_value_path 收集有限标量，同一目标在一轮声明前的更新取最新值。键盘连续增减基于未消费的值累积。Slider 消费事件后直接更新借用值，按通常的同帧稳定化规则再声明一次；副作用只写在 response().changed 条件内。

开发协议在原有 clicks 外增加可选 values，使用逻辑路径而非 arena handle。ReloadView 回放数值输入，模块恢复应用状态并消费事件。静态调用用 component 的生成身份，动态列表仍需业务 key。正确标识控件时，卸载、禁用或路径失效的旧事件不会应用给其他控件。没有把闭包或 &mut f64 留在 retained arena 中。

## 例子和验证

Gallery → Motion 的 Exposure 控制实时 WGSL 的 uniform。scripts/check-modal-reload.py 在打开的 modal 内通过 headless SetValue 改变滑块，校验步进、应用状态、shader payload 和真实 cdylib 替换后的保留。

```sh
cargo run -p plico-gallery
cargo test -p plico-vello --test slider
cargo test -p plico-core --test slider
```

只读参考 MyGO ui/base.go / ui/widgets.go 的 SliderBase、连续/步进数值绑定，以及 Wabou slider.tsx 的键盘、orientation、reversed 和范围规范化。未修改原仓库，也未复制其实现。测试覆盖拖动捕获、键盘累积、AT 设值与增减、禁用继承、端点、极端步长、非法配置、同帧值和热重载协议。语义桥接测试不等同于实机屏幕阅读器验收。
