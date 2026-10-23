# 注入式逻辑时钟

时钟属于宿主，应用和组件不直接读取 Instant。Clock trait 只返回从逻辑起点开始的 Duration：

```rust
use plico_core::{Clock, VirtualClock};
use plico_vello::Source;
use std::time::Duration;

let clock = VirtualClock::default();
let mut source = Source::with_clock(gallery, clock.clone());
clock.advance(Duration::from_millis(100))?;
// 下一次 source.build_frame 使用推进后的时间，无需 sleep。
```

默认 Source::new 使用 SystemClock；外部可实现 Clock 注入时间，也可传入 Arc<T: Clock>。外部游戏循环、Bevy 或回放系统可以持有/更新共享时间源。尚未提供 Bevy 插件。Clock 要求单调递增；VirtualClock 的 set 拒绝回退，advance 检查溢出，clone 共享进度。暂停时停止推进；缩放和 tick 步长由宿主决定。暂停逻辑时间不等于自动暂停 native 重绘调度，调度仍由宿主管理。

## 帧快照

Ui::time 返回 FrameTime { now, delta }，也可用 ui.now() / ui.delta()。宿主在帧开始采样一次，同帧事件稳定化的所有声明使用同一份快照，不能因为重复声明而推进动画。第一帧 delta=0；后续 delta 是相邻 build 帧的逻辑时间差。事件前提交也使用该时间域。

核心 Engine 本身没有隐藏系统时钟，默认时间为零；独立核心测试可 set_time。Source 仍保留 build_frame_at / prepare_for_event_at 作为 Instant 测试入口，native Instant 仅是后端已有 motion 的内部桥接，UI/协议都使用 Duration。

## 下一帧与同帧收敛

```rust
#[component]
fn timer(ui: &mut Ui<'_>) {
    ui.text(format!("{} ms", ui.now().as_millis()));
    if ui.now() < Duration::from_secs(1) {
        ui.request_animation_frame();
    }
}
```

Changes::ANIMATE 请求未来一帧，不导致同帧稳定化循环。Changes::VIEW 则请求同帧再声明；点击消费自动设置它，普通应用不维护 changed。计时逻辑结束后不再请求 ANIMATE，宿主恢复休眠。

同一时间源还驱动主题转场和 layout/style MotionArena。时间不是每个控件自己的 now，也不会因 reload 单独重置。App 协议请求携带 FrameTime，响应携带 requests flags；ReloadView 仅在事件、reload 或应用请求动画时调用模块，普通 hover/宿主 tween 帧继续复用应用声明。运行中的模块报错时保留上一画面并停止它的连续帧请求，避免错误循环；新事件或成功 reload 可再次驱动它。

长时间 idle 后开始动画会先更新时间基线，避免把 idle 时间计入新动画。若整个宿主停顿，下一帧按经过的逻辑时间快进，这是 elapsed-time 动画语义。VirtualClock 可以通过手动推进实现完全可重复的实验。

## 验证与尚未接入的部分

clock 测试验证外部 Duration 时钟、共享 virtual clock、暂停、回退/溢出拒绝、同帧时间一致、UI 未来帧请求及布局 tween。应用协议测试经过 JSON + Stabby dispatch 验证 time 与 ANIMATE 的传输。尚未接入通用定时器/异步 executor、按逻辑时间的 deadline 队列、OS reduced-motion 或 Bevy Time 桥接；这些应共享这个时钟接口。

可运行 `cargo run -p plico-vello --example virtual-clock` 观察 0 → 100 → 100（暂停）→ 200 ms，最后 next_frame=false。整个实验没有 sleep、线程或窗口。
