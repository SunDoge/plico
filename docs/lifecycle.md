# 动画、异步与热更新

本文主要记录原 view-repr 模拟实验的结论，以下系统尚未迁入 Plico。这里是后续实现的约束，不是可调用的 Plico API 文档。

## 时间与动画

期望 UI 每次声明目标值，运行时按稳定 key 保留轨迹。相同目标不重新启动；中途改变目标从当前采样值衔接。动画应与普通 UI 一样表达为字号、宽度、颜色、位置或透明度等值的变化。

原实验用共享模拟时钟验证：一帧及其稳定化 passes 观察同一时间，retarget 保持数值连续，动画结束后不继续请求帧，快照恢复保留曲线进度。插值曲线只保证位置/数值连续，未证明速度连续、spring 或任意物理动画。

迁移时需继续分开：

- layout 动画会触发布局；transform/opacity 动画可能绕过部分布局，但需要专门验证。
- 请求下一帧与立即持续重绘不同；能提供截止时间就允许宿主休眠。
- 卸载时回收轨迹；重新挂载是否重播由明确策略决定。
- snapshot 保存相对进度，不保存进程内绝对 Instant。
- reduced motion、有限与无限动画需要不同的调度约定。

大量动画实验必须区分声明/布局开销与 GPU 开销。原实验的 10,000 个动画只是模拟时间和 display list 更新，不是 10,000 个屏幕可见动画的 FPS 证明。

## 异步任务

目标是在 view 中声明任务的 key 与依赖输入，读取 Pending / Ready / Failed，同时不让每次重新声明重复发起任务。

原实验使用 SimJob 模拟完成时间，验证去重、超时、取消、卸载、乱序完成和重载重启。它没有真实 Future executor、线程池、网络或 IO。同步构造模拟返回值不能被当成异步执行实现。

未来真实执行需要：

1. 任务描述在声明时产生，仅最终有效 pass 提交后启动，推测分支不启动副作用。
2. 请求带 key、request id 和 generation；完成消息回 UI 线程，在边界应用。
3. 依赖变化或卸载使旧请求失效；即使 worker 无法取消，旧结果也不得覆盖新状态。
4. UI 线程独占布局与渲染；后台通过队列和 wake 通知交付数据。
5. 同依赖下任务逻辑变更要能手动增加版本，不能仅凭输入相等永久复用结果。
6. 超时与取消不承诺撤销已经发生的外部副作用。

## Best effort hot reload

目标是修改 UI 代码后尽量保留状态；无法恢复的槽位单独重置，整体协议不兼容时可以重置。用户已接受 best effort，不需要假装任意 Rust 值都可迁移。

原实验采用独立进程和 JSON checkpoint，跨边界不传 Rust ABI、闭包、指针或 trait object。候选版本编译并 smoke test 成功后再激活；失败保留上一可用版本。字段按 key、类型标记、schema version 恢复，反序列化失败则报告重置。

Plico 未来可重新选择进程或 hotpatch 方案，但必须保留清晰的数据边界。异步 Future 和执行器内部状态不恢复：已完成的可序列化结果可复用，未完成任务重新计划，旧 generation 的结果丢弃。

原热更新进程实验每次请求重建时钟，并未提供可持续播放真实动画的常驻事件循环。迁移时需要把运行时驻留、帧调度、平台输入和代码更新一起验证。

## Plico 已实现的开发热更新

本文前述 view-repr 进程方案是历史实验。Plico 现在实现了另一条可执行链路：常驻真实 renderer + 可替换 app cdylib，使用 notify、libloading/Stabby 与 Serde，不是进程请求模拟。

`App` 提供 Default、Serialize、DeserializeOwned 和 show；host 传入已有状态，模块重建临时声明 Engine、消费逻辑路径事件并稳定化后返回 UI 描述。兼容状态恢复，失败则整个 App 重置；逻辑 key 复用 host 节点。compile、panic、协议与 ABI 错误不激活候选。app 模块不保存未来回调，代码映像保留至 host 退出。

该实现已成为 plico dev 的默认行为。详见 [CLI 文档](cli.md) 和 真实重载证据（本地生成：`../reports/hotreload/report.json`）。动画/真实异步执行器仍未迁入，不能由热更新成功推断它们已有实现。

## 当前属性动画

显式尺寸、间距与基础 paint 过渡已接入正式宿主，使用内置 plico-motion 和模拟时钟；实际能力和限制见 [layout-style.md](layout-style.md)。原实验的任务系统、自动状态追踪与通用动画 API 尚未因此迁入。

## 正式运行时的 clock

Clock/FrameTime 已接入核心、宿主、动态应用和动画请求，可从外部注入 VirtualClock 或游戏时间源。同帧重声明不会推进时间。详见 [clock.md](clock.md)；通用定时器与任务 executor 仍未迁入。
