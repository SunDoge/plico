# 动画、异步与开发生命周期

本文区分当前可调用能力和历史实验约束。Plico 使用静态编译重启与确定性场景预览，不提供桌面动态库热更新。

## 时间与动画

UI 可以每次声明目标值，运行时按声明点、组件 scope 和显式 key 保留轨迹。`UiContext::animate` / `animate_with` 提供有限曲线，`animate_to` / `spring` 提供 delay、速度继承和解析弹簧；相同目标不重新启动，中途改变目标从当前采样值和速度衔接。

正式实现使用共享注入时钟；一帧及其稳定化 passes 观察同一时间。虚拟时钟回归覆盖 retarget 连续、tween 到 spring 的速度交接、delay、精确停靠、reduced-motion 和卸载释放。动画 channel 是瞬态 UI 生命周期状态，不写入 checkpoint；重新挂载从首个目标开始。

迁移时需继续分开：

- layout 动画会触发布局；transform/opacity 动画可能绕过部分布局，但需要专门验证。
- 连续动画请求显示节奏声明帧；低频计时使用 deadline，避免无意义的 vsync loop。
- 卸载时回收轨迹；重新挂载是否重播由明确策略决定。
- snapshot 不保存动画轨迹或进程内绝对时间，只保存应用自己的持久目标状态。
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

## 静态开发循环

`plico dev` 监听源码，编译成功后重启普通 Rust executable；编译失败时保留旧进程。应用通过正常持久化恢复需要跨重启保留的数据。JSON/TypeScript recipe 使用应用定义的 preview 配置和虚拟时钟复现状态，不序列化任意 App、Future、闭包或宿主资源。桌面 cdylib、Stabby ABI 和进程内代码替换已经移除，不重新作为默认路径。详见 [场景预览](preview.md) 与 [CLI](cli.md)。

## 当前属性动画

显式尺寸、间距与基础 paint 过渡由宿主的 `plico-motion` 管理；声明值动画由 `UiContext` 的 MyGO 风格 channel 管理。前者可跳过应用声明，后者适合会参与应用组合或布局的数值。实际属性范围见 [layout-style.md](layout-style.md)。异步任务仍由宿主服务执行，通过快照和消息回到 UI；框架不自动追踪任意 Rust 依赖或 Future。

## 正式运行时的 clock

Clock/FrameTime 已接入核心、宿主和动画请求，可从外部注入 VirtualClock 或游戏时间源。同帧重声明不会推进时间。详见 [clock.md](clock.md)；通用任务 executor 不进入 UI 核心。
